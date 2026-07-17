use cosmwasm_std::{
    ensure, from_json, to_json_binary, Addr, Binary, CosmosMsg, DepsMut, Env, MessageInfo,
    Response, SubMsg, Uint256, WasmMsg,
};
use euclid::{
    error::ContractError,
    events::{
        receive_acknowledgement_event, receive_packet_event, send_packet_encoded_event,
        EUCLID_RECEIVE_PACKET_EVENT, EUCLID_SEND_PACKET_ENCODED_EVENT,
        EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT,
    },
    msgs::{factory::ExecuteMsg, hook::EuclidAcknowledgement},
    timeout::get_timeout,
};
use euclid_encoding::{Encoding, PROTOCOL_VERSION};
use euclid_ibc::wire::{
    envelope::{factory::FactoryReceiveMsg, make_ack_fail, router::RouterReceiveMsg},
    transcode,
};

use crate::{
    ibc::{ack_and_timeout, receive},
    rate_limit::ensure_rate_limit_exceeded,
    relay_state::{
        create_pending_packet_and_update_sequence, map_encoding_err,
        remove_pending_packet_and_decrement_count, InFlightReceive,
        CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS, IN_FLIGHT_RECEIVE,
    },
    reply::CROSS_CHAIN_RECEIVE_REPLY_ID,
    state::STATE,
};

/**
 * Always run by contract itself to trigger send packet event and also increment sequence count.
 * This creates a new event for each send packet.
 */
pub fn execute_send_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: Binary,
    timeout: Option<u64>,
    ack_response: Option<Binary>,
    sender: Addr,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // Only contract can call this function internally
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    let response = Response::new();

    let factory_state = STATE.load(deps.storage)?;

    ensure_rate_limit_exceeded(&deps, sender.clone())?;

    // One leg, one encoding: the factory's own leg is always Json today (a
    // CosmWasm factory sits on a Cosmos or Native chain).
    let encoding = Encoding::Json;

    // The factory sends RouterReceiveMsg, so this is the router
    // encoder; the Json arm is a byte passthrough of the domain JSON.
    // Encode before the pending packet write so the exact emitted wire bytes are
    // committed to storage for the ack byte check (Amendment B).
    let wire_msg =
        transcode::encode_router_receive_from_json(&msg, encoding).map_err(map_encoding_err)?;

    let sequence = create_pending_packet_and_update_sequence(
        deps.storage,
        &msg,
        ack_response,
        &sender,
        encoding.as_u8(),
        wire_msg.clone(),
    )?;

    let source_port = format!("{}.{}", *factory_state.chain_uid, env.contract.address);

    let destination_port = format!("vsl.{}", factory_state.router_contract);

    let timeout = get_timeout(timeout)?;
    let timeout = env.block.time.plus_seconds(timeout).seconds();

    let event = send_packet_encoded_event(
        &euclid_encoding::repr::to_transport_string(&wire_msg, encoding)
            .map_err(map_encoding_err)?,
        sequence,
        &source_port,
        &destination_port,
        timeout,
        "cosmos", // the hub is the destination; the hardcoded value stays correct
        PROTOCOL_VERSION,
        encoding.as_u8(),
    );

    Ok(response
        .add_attribute("action", EUCLID_SEND_PACKET_ENCODED_EVENT)
        .add_event(event))
}

// Always run by relayer to trigger a receive packeg event. At the end of receive, there will be a write acknowledgement event
#[allow(clippy::too_many_arguments)]
pub fn execute_receive_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: String,
    sequence: u128,
    source_port: String,
    destination_port: String,
    timeout: u64,
    encoding: u8,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == state.relayer_contract,
        ContractError::Unauthorized {}
    );
    ensure!(
        destination_port == format!("{}.{}", state.chain_uid.as_str(), env.contract.address),
        ContractError::new("Invalid destination port")
    );
    ensure!(
        source_port == format!("vsl.{router}", router = state.router_contract),
        ContractError::new("Invalid source port")
    );

    let processed_sequence_key = CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS.key(sequence);
    ensure!(
        !processed_sequence_key.has(deps.storage),
        ContractError::Generic {
            err: "Processed sequence already exists".to_string()
        }
    );
    // Save the processed sequence to avoid duplicate events
    processed_sequence_key.save(deps.storage, &Uint256::from(env.block.height))?;

    let receive_packet_event = receive_packet_event(sequence, &source_port, &destination_port);

    // Strict decode of the declared leg encoding; no fallback probing. The msg
    // arrives as a String in the transport representation; recover the wire
    // bytes first (raw JSON text bytes when Json, 0x hex decode when Abi).
    let encoding = Encoding::from_u8(encoding).map_err(map_encoding_err)?;
    let msg_bytes: Binary = euclid_encoding::repr::from_transport_string(&msg, encoding)
        .map_err(map_encoding_err)?
        .into();
    let received: FactoryReceiveMsg =
        transcode::decode_factory_receive(&msg_bytes, encoding).map_err(map_encoding_err)?;
    let json = to_json_binary(&received)?;
    let tx_id = received.get_tx_id();

    // Record the receive context for the reply, which transcodes the ack back
    // onto the leg encoding keyed by the send msg's wire tag and emits the
    // single complete acknowledgement event. No acknowledgement event is
    // emitted here; only receive_packet_event fires at receive time.
    IN_FLIGHT_RECEIVE.save(
        deps.storage,
        &InFlightReceive {
            source_port: source_port.clone(),
            destination_port: destination_port.clone(),
            msg: msg_bytes,
            sequence,
            encoding: encoding.as_u8(),
            wire_tag: received.wire_tag(),
        },
    )?;

    // Internal plumbing stays domain JSON regardless of the wire encoding.
    let internal_msg = ExecuteMsg::ReceivePacketInternalCallback { msg: json, timeout };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, CROSS_CHAIN_RECEIVE_REPLY_ID);

    Ok(Response::new()
        .add_attribute("method", EUCLID_RECEIVE_PACKET_EVENT)
        .add_attribute("action", EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT)
        .add_attribute("tx_id", tx_id)
        .set_data(make_ack_fail("default_fail".to_string())?)
        .add_event(receive_packet_event)
        .add_submessage(sub_msg))
}

// Always run by contract itself to trigger a receive packet event. This is needed because we should never fail receive packet event
pub fn execute_receive_packet_internal_callback(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Binary,
    timeout: u64,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );
    ensure!(
        timeout >= env.block.time.seconds(),
        ContractError::PacketTimedOut {
            timeout,
            block_time: env.block.time.seconds()
        }
    );
    let msg: FactoryReceiveMsg = from_json(msg)?;
    receive::reusable_internal_call(deps, env, msg)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_receive_acknowledgement(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    msg: String,
    sequence: u128,
    source_port: String,
    destination_port: String,
    ack: String,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == state.relayer_contract,
        ContractError::Unauthorized {}
    );
    ensure!(
        destination_port == format!("{}.{}", state.chain_uid.as_str(), env.contract.address),
        ContractError::new("Invalid destination port")
    );
    ensure!(
        source_port == format!("vsl.{router}", router = state.router_contract),
        ContractError::new("Invalid source port")
    );

    let (existing_request, sender) =
        remove_pending_packet_and_decrement_count(deps.storage, sequence)?;

    // Parse after load: the String parameters are opaque until the pending
    // packet is loaded; the representation comes from the stored encoding.
    let encoding = Encoding::from_u8(existing_request.encoding).map_err(map_encoding_err)?;
    let msg_bytes: Binary = euclid_encoding::repr::from_transport_string(&msg, encoding)
        .map_err(map_encoding_err)?
        .into();
    let ack_bytes: Binary = euclid_encoding::repr::from_transport_string(&ack, encoding)
        .map_err(map_encoding_err)?
        .into();
    // Amendment B byte check, the CosmWasm mirror of Solidity PacketMsgMismatch:
    // wire bytes are canonical, the relayer must return exactly what was sent.
    ensure!(
        msg_bytes == existing_request.wire_msg,
        ContractError::PacketMsgMismatch { sequence }
    );

    // Dispatch keys off the locally-stored original message, never the
    // supplied bytes (defense in depth).
    let msg: RouterReceiveMsg = from_json(&existing_request.original_msg)?;

    // The ack rides the wire in the leg encoding recorded at send time;
    // transcode it back to the internal JSON form keyed by the send tag.
    let ack_json = transcode::router_ack_wire_to_json(msg.wire_tag(), &ack_bytes, encoding)
        .map_err(map_encoding_err)?;

    let response = ack_and_timeout::reusable_internal_ack_call(
        deps,
        env,
        msg,
        ack_json.clone(),
        state.is_native,
    )?;
    let ack_event = receive_acknowledgement_event(sequence, &source_port, &destination_port);
    let mut response = response.add_event(ack_event);

    if let Some(ack_response) = existing_request.ack_response {
        let is_contract = deps
            .querier
            .query_wasm_contract_info(sender.to_string())
            .is_ok();
        if is_contract {
            // The hook keeps receiving the JSON ack (internal contract data
            // shape unchanged).
            let ack_hook_msg = EuclidAcknowledgement {
                ack: ack_json,
                msg: ack_response,
            }
            .to_receiver_msg();
            let msg = WasmMsg::Execute {
                contract_addr: sender.to_string(),
                msg: ack_hook_msg?,
                funds: vec![],
            };
            // This is a never reply message, so we don't need to wait for a response
            response = response.add_submessage(SubMsg::reply_never(msg));
        }
    }

    Ok(response)
}

pub fn execute_native_receive_callback(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Binary,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let msg: FactoryReceiveMsg = from_json(msg)?;
    let state = STATE.load(deps.storage)?;

    // Only native chains can directly use this messages
    ensure!(
        state.is_native,
        ContractError::new("Only native chains can execute this message")
    );

    // Only router contract can execute this message
    ensure!(
        state.router_contract == info.sender.to_string(),
        ContractError::new("Only router contract can execute this message")
    );
    receive::reusable_internal_call(deps, env, msg)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        attr,
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, Addr, Binary, CosmosMsg, Env, Uint256, WasmMsg,
    };
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        events::{EUCLID_SEND_PACKET_ENCODED_EVENT, EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT},
        msgs::factory::ExecuteMsg,
        token::{Token, TokenType, TokenWithDenom},
    };
    use euclid_encoding::{Encoding, PROTOCOL_VERSION};
    use euclid_ibc::wire::msgs::RegisterDenomSendMsg;
    use euclid_ibc::wire::msgs::ReleaseEscrowSendMsg;
    use euclid_ibc::{
        wire::envelope::factory::FactoryReceiveMsg, wire::envelope::router::RouterReceiveMsg,
    };
    use rstest::*;

    use crate::{
        contract::execute,
        relay_state::{
            create_pending_packet_and_update_sequence, InFlightReceive,
            CROSS_CHAIN_PENDING_SEND_PACKETS, CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS,
            IN_FLIGHT_RECEIVE,
        },
        testing::helpers::{init, MockDeps, TEST_CHAIN_UID, TEST_RELAYER, TEST_ROUTER},
    };

    fn make_deps() -> MockDeps {
        let mut deps = mock_dependencies();
        init(&mut deps);
        deps
    }

    fn event_attr(event: &cosmwasm_std::Event, key: &str) -> String {
        event
            .attributes
            .iter()
            .find(|a| a.key == key)
            .unwrap_or_else(|| panic!("missing attribute {key}"))
            .value
            .clone()
    }

    /// The factory's own port: `{chain_uid}.{factory_address}`.
    fn factory_port(env: &Env) -> String {
        format!("{}.{}", TEST_CHAIN_UID, env.contract.address)
    }

    /// The hub's port: `vsl.{router_address}`.
    fn hub_port() -> String {
        format!("vsl.{TEST_ROUTER}")
    }

    /// Outbound domain msg (the factory sends `RouterReceiveMsg`).
    fn register_denom_msg() -> RouterReceiveMsg {
        RouterReceiveMsg::RegisterDenom(RegisterDenomSendMsg {
            sender: CrossChainUser::new(
                ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
                "user-addr".to_string(),
            ),
            tx_id: "tx-send-1".to_string(),
            token: TokenWithDenom {
                token: Token::create("abc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uabc".to_string(),
                    decimals: None,
                },
            },
        })
    }

    /// Inbound domain msg (the factory receives `FactoryReceiveMsg`).
    fn release_escrow_msg() -> FactoryReceiveMsg {
        FactoryReceiveMsg::ReleaseEscrow(ReleaseEscrowSendMsg {
            sender: CrossChainUser::new(
                ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
                "factory-addr".to_string(),
            ),
            token: Token::create("abc".to_string()).unwrap(),
            recipient: "recipient-addr".to_string(),
            amount: Uint256::from(1_000u128),
            denom: TokenType::Native {
                denom: "uabc".to_string(),
                decimals: None,
            },
            forwarding_message: None,
            tx_id: "tx1".to_string(),
        })
    }

    // -----------------------------------------------------------------------
    // Relay handlers: unauthorized callers (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::send_packet(
        Addr::unchecked("external_caller"),
        ExecuteMsg::SendPacket {
            msg: Binary::default(),
            sender: Addr::unchecked("sender"),
            timeout: None,
            ack_response: None,
        },
    )]
    #[case::receive_packet(
        Addr::unchecked("attacker"),
        ExecuteMsg::ReceivePacket {
            source_port: "vsl.router_contract".to_string(),
            destination_port: "testchain.contract".to_string(),
            msg: String::new(),
            sequence: 0,
            timeout: u64::MAX,
            encoding: 0,
        },
    )]
    #[case::acknowledge_packet(
        Addr::unchecked("attacker"),
        ExecuteMsg::AcknowledgePacket {
            source_port: "vsl.router_contract".to_string(),
            destination_port: "testchain.contract".to_string(),
            msg: String::new(),
            sequence: 0,
            ack: String::new(),
        },
    )]
    #[case::receive_packet_internal_callback(
        Addr::unchecked("external"),
        ExecuteMsg::ReceivePacketInternalCallback {
            msg: Binary::default(),
            timeout: u64::MAX,
        },
    )]
    fn test_relay_handler_rejects_unauthorized_caller(
        #[case] sender: Addr,
        #[case] msg: ExecuteMsg,
    ) {
        let mut deps = make_deps();
        let res = execute(deps.as_mut(), mock_env(), message_info(&sender, &[]), msg);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // ReceivePacket: port & sequence validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_receive_packet_invalid_destination_port() {
        let mut deps = make_deps();
        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: hub_port(),
                destination_port: "wrong.something".to_string(),
                msg: String::new(),
                sequence: 0,
                timeout: u64::MAX,
                encoding: 0,
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Invalid destination port")
        );
    }

    #[test]
    fn test_receive_packet_invalid_source_port() {
        let mut deps = make_deps();
        let env = mock_env();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: "vsl.wrongrouter".to_string(),
                destination_port: factory_port(&env),
                msg: String::new(),
                sequence: 0,
                timeout: u64::MAX,
                encoding: 0,
            },
        );
        assert_eq!(res.unwrap_err(), ContractError::new("Invalid source port"));
    }

    #[test]
    fn test_receive_packet_duplicate_sequence() {
        let mut deps = make_deps();
        let env = mock_env();
        CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS
            .save(deps.as_mut().storage, 0u128, &Uint256::from(1u64))
            .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                msg: String::new(),
                sequence: 0,
                timeout: u64::MAX,
                encoding: 0,
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::Generic {
                err: "Processed sequence already exists".to_string()
            }
        );
    }

    // -----------------------------------------------------------------------
    // AcknowledgePacket: port validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_acknowledge_packet_invalid_destination_port() {
        let mut deps = make_deps();
        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: hub_port(),
                destination_port: "wrong.something".to_string(),
                msg: String::new(),
                sequence: 0,
                ack: String::new(),
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Invalid destination port")
        );
    }

    // -----------------------------------------------------------------------
    // SendPacket: encoded event emission (the factory leg is always Json)
    // -----------------------------------------------------------------------

    #[test]
    fn test_send_packet_emits_encoded_event_json_leg() {
        let mut deps = make_deps();
        let env = mock_env();
        let sender = deps.api.addr_make("alice");
        let domain_msg = register_denom_msg();
        let msg_json = to_json_binary(&domain_msg).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&env.contract.address, &[]),
            ExecuteMsg::SendPacket {
                msg: msg_json.clone(),
                sender: sender.clone(),
                timeout: None,
                ack_response: None,
            },
        )
        .unwrap();

        // Old event/action names must be fully replaced.
        assert_eq!(
            res.attributes[0],
            attr("action", EUCLID_SEND_PACKET_ENCODED_EVENT)
        );
        assert!(res.events.iter().all(|e| e.ty != "euclid-send-packet"));

        let event = res
            .events
            .iter()
            .find(|e| e.ty == EUCLID_SEND_PACKET_ENCODED_EVENT)
            .expect("missing encoded send event");

        // Canonical cross-VM attribute order, matching the Solidity SendPacket
        // event field order.
        let keys: Vec<&str> = event.attributes.iter().map(|a| a.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "msg",
                "sequence",
                "source_port",
                "destination_port",
                "timeout",
                "destination_chain_type",
                "version",
                "encoding"
            ]
        );

        assert_eq!(event_attr(event, "source_port"), factory_port(&env));
        assert_eq!(event_attr(event, "destination_port"), hub_port());
        assert_eq!(event_attr(event, "sequence"), "0");
        // The hub is the destination; the hardcoded chain type stays correct.
        assert_eq!(event_attr(event, "destination_chain_type"), "cosmos");
        assert_eq!(event_attr(event, "version"), PROTOCOL_VERSION);
        assert_eq!(event_attr(event, "encoding"), "0");
        // Json leg: raw JSON text, byte-identical to the domain JSON.
        assert_eq!(
            event_attr(event, "msg"),
            String::from_utf8(msg_json.to_vec()).unwrap()
        );

        // The stored pending packet records the leg encoding; the original msg
        // stays domain JSON for the ack path.
        let pending = CROSS_CHAIN_PENDING_SEND_PACKETS
            .load(deps.as_ref().storage, 0u128)
            .unwrap();
        assert_eq!(pending.encoding, Encoding::Json.as_u8());
        assert_eq!(pending.original_msg, msg_json);

        // The stored wire_msg is exactly the bytes emitted in the send event.
        // Compute the expected bytes independently through the transcode encoder
        // (the factory sends RouterReceiveMsg values) rather than parsing the
        // event attribute back.
        let expected_wire =
            euclid_ibc::wire::transcode::encode_router_receive_from_json(&msg_json, Encoding::Json)
                .expect("router receive encode");
        assert_eq!(pending.wire_msg, expected_wire);
        // On the Json leg the wire bytes duplicate the domain JSON.
        assert_eq!(pending.wire_msg, msg_json);
    }

    // -----------------------------------------------------------------------
    // ReceivePacket: no acknowledgement event at receive time + in-flight
    // receive state carrying the full context
    // -----------------------------------------------------------------------

    #[test]
    fn test_receive_packet_json_emits_no_ack_event_and_saves_in_flight() {
        let mut deps = make_deps();
        let env = mock_env();
        let domain_msg = release_escrow_msg();
        let msg_json = to_json_binary(&domain_msg).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                // Json leg: the transport form is the raw JSON text.
                msg: String::from_utf8(msg_json.to_vec()).unwrap(),
                sequence: 5,
                timeout: u64::MAX,
                encoding: 0,
            },
        )
        .unwrap();

        // tx_id now comes from the strict decode, not the lossy peek.
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "tx_id")
                .unwrap()
                .value,
            "tx1"
        );

        // Only receive_packet_event is emitted at receive time; the complete
        // acknowledgement event comes from the reply handler.
        assert!(res
            .events
            .iter()
            .all(|e| e.ty != EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT));
        assert!(res
            .events
            .iter()
            .any(|e| e.ty == euclid::events::EUCLID_RECEIVE_PACKET_EVENT));

        // In-flight receive context saved for the reply, ports as received
        // (no chain_uid on the factory side; the counterparty is always the
        // hub).
        let ctx = IN_FLIGHT_RECEIVE.load(deps.as_ref().storage).unwrap();
        assert_eq!(
            ctx,
            InFlightReceive {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                msg: msg_json.clone(),
                sequence: 5,
                encoding: 0,
                wire_tag: 1, // TAG_RELEASE_ESCROW
            }
        );

        // The internal callback carries the domain JSON.
        let sub = res.messages.first().expect("missing internal callback");
        match &sub.msg {
            CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                let parsed: ExecuteMsg = cosmwasm_std::from_json(msg).unwrap();
                match parsed {
                    ExecuteMsg::ReceivePacketInternalCallback { msg, .. } => {
                        assert_eq!(msg, msg_json);
                    }
                    other => panic!("unexpected internal msg: {other:?}"),
                }
            }
            other => panic!("expected wasm execute, got {other:?}"),
        }
    }

    #[test]
    fn test_receive_packet_abi_decodes_and_saves_wire_tag() {
        let mut deps = make_deps();
        let env = mock_env();
        let domain_msg = release_escrow_msg();
        // Typed encode needs no transcode helper; the sample is the wire enum.
        let wire = Binary::from(euclid_encoding::encode(&domain_msg, Encoding::Abi).unwrap());

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                // Abi leg: the transport form is 0x lowercase hex.
                msg: euclid_encoding::repr::to_transport_string(&wire, Encoding::Abi).unwrap(),
                sequence: 3,
                timeout: u64::MAX,
                encoding: 1,
            },
        )
        .unwrap();

        // No acknowledgement event at receive time.
        assert!(res
            .events
            .iter()
            .all(|e| e.ty != EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT));

        let ctx = IN_FLIGHT_RECEIVE.load(deps.as_ref().storage).unwrap();
        // The wire bytes are stored as received for the single shot event.
        assert_eq!(ctx.msg, wire);
        assert_eq!(ctx.sequence, 3);
        assert_eq!(ctx.encoding, 1);
        assert_eq!(ctx.wire_tag, 1); // TAG_RELEASE_ESCROW

        // Internal plumbing stays domain JSON regardless of the wire encoding.
        let expected_json = to_json_binary(&domain_msg).unwrap();
        let sub = res.messages.first().expect("missing internal callback");
        match &sub.msg {
            CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                match cosmwasm_std::from_json::<ExecuteMsg>(msg).unwrap() {
                    ExecuteMsg::ReceivePacketInternalCallback { msg, .. } => {
                        assert_eq!(msg, expected_json);
                    }
                    other => panic!("unexpected internal msg: {other:?}"),
                }
            }
            other => panic!("expected wasm execute, got {other:?}"),
        }
    }

    #[rstest]
    #[case::unknown_encoding_tag(2, String::new())]
    #[case::undecodable_msg(0, String::new())]
    #[case::abi_msg_not_hex(1, "not-hex".to_string())]
    fn test_receive_packet_strict_decode_errors(#[case] encoding: u8, #[case] msg: String) {
        let mut deps = make_deps();
        let env = mock_env();
        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                msg,
                sequence: 0,
                timeout: u64::MAX,
                encoding,
            },
        );
        assert!(res.is_err(), "strict decode must reject the packet");
    }

    // -----------------------------------------------------------------------
    // AcknowledgePacket: ack transcode keyed by the stored pending packet
    // -----------------------------------------------------------------------

    #[test]
    fn test_acknowledge_packet_abi_leg_rejects_undecodable_ack() {
        let mut deps = make_deps();
        let env = mock_env();
        let sender = deps.api.addr_make("alice");

        // Pending packet stored with the Abi leg encoding.
        let msg_json = to_json_binary(&register_denom_msg()).unwrap();
        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &msg_json,
            None,
            &sender,
            Encoding::Abi.as_u8(),
            Binary::default(),
        )
        .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                // Valid Abi transport form (0x hex) so the failure comes from
                // the ack transcode, not the representation decode.
                msg: euclid_encoding::repr::to_transport_string(&msg_json, Encoding::Abi).unwrap(),
                sequence: 0,
                ack: euclid_encoding::repr::to_transport_string(b"not-abi", Encoding::Abi).unwrap(),
            },
        );
        assert!(res.is_err(), "abi leg must reject an undecodable wire ack");
    }

    // -----------------------------------------------------------------------
    // AcknowledgePacket: Amendment B byte check against the stored wire bytes
    // -----------------------------------------------------------------------

    /// A RegisterDenom send msg with a validatable sender, so its ack path
    /// (which `addr_validate`s the sender) round-trips through dispatch.
    fn register_denom_msg_from(sender: &Addr, tx_id: &str) -> RouterReceiveMsg {
        RouterReceiveMsg::RegisterDenom(RegisterDenomSendMsg {
            sender: CrossChainUser::new(
                ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
                sender.to_string(),
            ),
            tx_id: tx_id.to_string(),
            token: TokenWithDenom {
                token: Token::create("abc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uabc".to_string(),
                    decimals: None,
                },
            },
        })
    }

    /// Send `domain_msg` on the factory's Json leg and return the transport
    /// `msg` string emitted in the send event (raw JSON text).
    fn send_and_capture_msg(
        deps: &mut MockDeps,
        env: &Env,
        domain_msg: &RouterReceiveMsg,
    ) -> String {
        let msg_json = to_json_binary(domain_msg).unwrap();
        let sender = deps.api.addr_make("alice");
        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&env.contract.address, &[]),
            ExecuteMsg::SendPacket {
                msg: msg_json,
                sender,
                timeout: None,
                ack_response: None,
            },
        )
        .unwrap();
        let event = res
            .events
            .iter()
            .find(|e| e.ty == EUCLID_SEND_PACKET_ENCODED_EVENT)
            .expect("missing encoded send event");
        event_attr(event, "msg")
    }

    #[test]
    fn test_acknowledge_packet_positive_roundtrip_dispatches() {
        use crate::state::{
            DenomRegisterDeregisterRequest, PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS,
        };

        let mut deps = make_deps();
        let env = mock_env();
        let sender = deps.api.addr_make("alice");
        let domain_msg = register_denom_msg_from(&sender, "tx-pos");
        let msg = send_and_capture_msg(&mut deps, &env, &domain_msg);

        // Seed the pending denom request the RegisterDenom ack path removes.
        PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
            .save(
                deps.as_mut().storage,
                (sender.clone(), "tx-pos".to_string()),
                &DenomRegisterDeregisterRequest {
                    tx_id: "tx-pos".to_string(),
                    sender: sender.clone(),
                    token: TokenWithDenom {
                        token: Token::create("abc".to_string()).unwrap(),
                        token_type: TokenType::Native {
                            denom: "uabc".to_string(),
                            decimals: None,
                        },
                    },
                },
            )
            .unwrap();

        // Exact send msg round-trips: the byte check passes and the ack
        // dispatches (RegisterDenom Error arm on a non-native factory -> Ok).
        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                msg,
                sequence: 0,
                ack: String::from_utf8(
                    euclid_ibc::wire::envelope::make_ack_fail("boom".to_string())
                        .unwrap()
                        .to_vec(),
                )
                .unwrap(),
            },
        );
        assert!(
            res.is_ok(),
            "exact msg must pass the byte check and dispatch"
        );
    }

    #[test]
    fn test_acknowledge_packet_tampered_msg_rejected() {
        let mut deps = make_deps();
        let env = mock_env();
        let sender = deps.api.addr_make("alice");
        let domain_msg = register_denom_msg_from(&sender, "tx-tamper");
        let msg = send_and_capture_msg(&mut deps, &env, &domain_msg);

        // Flip one byte of the transport msg so the recovered wire bytes differ
        // from the stored PendingPacket.wire_msg by exactly one byte.
        let mut tampered = msg.into_bytes();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let tampered = String::from_utf8(tampered).unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                msg: tampered,
                sequence: 0,
                ack: String::new(),
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::PacketMsgMismatch { sequence: 0 }
        );
    }

    #[test]
    fn test_acknowledge_packet_wrong_representation_rejected() {
        let mut deps = make_deps();
        let env = mock_env();
        let sender = deps.api.addr_make("alice");

        // Pending packet stored with the Abi leg encoding.
        let msg_json = to_json_binary(&register_denom_msg()).unwrap();
        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &msg_json,
            None,
            &sender,
            Encoding::Abi.as_u8(),
            Binary::default(),
        )
        .unwrap();

        // Abi leg but the msg is supplied as base64 (no 0x prefix). The
        // representation is only known after the pending packet is loaded, so
        // this fails on the parse-after-load, never reaching the byte check.
        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                msg: "eyJhIjoxfQ==".to_string(),
                sequence: 0,
                ack: String::new(),
            },
        );
        let err = res.unwrap_err();
        assert!(
            !matches!(err, ContractError::PacketMsgMismatch { .. }),
            "must fail on the representation decode, not the byte check"
        );
        assert!(
            err.to_string().contains("representation"),
            "expected an InvalidRepresentation mapped error, got {err}"
        );
    }

    #[test]
    fn test_acknowledge_packet_pre_upgrade_empty_wire_msg_rejected() {
        let mut deps = make_deps();
        let env = mock_env();
        let sender = deps.api.addr_make("alice");

        // Pre-upgrade packet: Json leg with an empty wire_msg (as serde(default)
        // yields for packets stored before Amendment B). A correct ack whose
        // recovered bytes are the real (non-empty) msg still fails the byte
        // check because the stored wire_msg is empty (drain before upgrade).
        let msg_json = to_json_binary(&register_denom_msg()).unwrap();
        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &msg_json,
            None,
            &sender,
            Encoding::Json.as_u8(),
            Binary::default(),
        )
        .unwrap();

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: hub_port(),
                destination_port: factory_port(&env),
                msg: String::from_utf8(msg_json.to_vec()).unwrap(),
                sequence: 0,
                ack: String::new(),
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::PacketMsgMismatch { sequence: 0 }
        );
    }

    // -----------------------------------------------------------------------
    // ReceivePacketInternalCallback: timeout
    // -----------------------------------------------------------------------

    #[test]
    fn test_receive_packet_internal_callback_timed_out() {
        let mut deps = make_deps();
        let env = mock_env();
        // timeout=0 is below mock_env block time (1571797419)
        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&env.contract.address, &[]),
            ExecuteMsg::ReceivePacketInternalCallback {
                msg: Binary::default(),
                timeout: 0,
            },
        );
        assert!(matches!(
            res.unwrap_err(),
            ContractError::PacketTimedOut { .. }
        ));
    }
}
