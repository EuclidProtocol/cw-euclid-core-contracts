use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, MessageInfo, Response,
    SubMsg, Uint256, WasmMsg,
};
use euclid::{
    chain::{Chain, ChainType, ChainUid},
    error::ContractError,
    events::{
        receive_acknowledgement_event, receive_packet_event, send_packet_encoded_event,
        EUCLID_RECEIVE_PACKET_EVENT, EUCLID_SEND_PACKET_ENCODED_EVENT,
        EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT,
    },
    msgs::router::ExecuteMsg,
    timeout::get_timeout,
};
use euclid_encoding::{Encoding, PROTOCOL_VERSION};
use euclid_ibc::wire::{
    envelope::{factory::FactoryReceiveMsg, make_ack_fail, router::RouterReceiveMsg},
    transcode,
};

use crate::{
    ibc::{ack_and_timeout, receive},
    relay_state::{
        create_pending_packet_and_update_sequence, map_encoding_err,
        remove_pending_packet_and_decrement_count, InFlightReceive,
        CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS, IN_FLIGHT_RECEIVE,
    },
    reply::CROSS_CHAIN_RECEIVE_REPLY_ID,
    state::{CHAIN_UID_TO_CHAIN, RELAYER_CONTRACT},
};

#[allow(clippy::too_many_arguments)]
pub fn execute_send_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    chain: Chain,
    msg: Binary,
    timeout: Option<u64>,
    ack_response: Option<Binary>,
    sender: String,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // Only contract can call this function internally
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    // One leg, one encoding: EVM and TVM legs speak canonical ABI, everything
    // else stays JSON.
    let encoding = match chain.chain_type {
        ChainType::Evm(_) | ChainType::Tvm(_) => Encoding::Abi,
        _ => Encoding::Json,
    };

    // The router sends FactoryReceiveMsg, so this is the factory
    // encoder; the Json arm is a byte passthrough of the domain JSON.
    // Encode before the pending packet write so the exact emitted wire bytes are
    // committed to storage for the ack byte check (Amendment B).
    let wire_msg =
        transcode::encode_factory_receive_from_json(&msg, encoding).map_err(map_encoding_err)?;

    let (chain_uid, sequence) = create_pending_packet_and_update_sequence(
        deps.storage,
        &chain,
        &msg,
        ack_response,
        &sender,
        encoding.as_u8(),
        wire_msg.clone(),
    )?;

    let source_port = format!("vsl.{}", env.contract.address.to_string().to_lowercase());
    let destination_port = format!(
        "{}.{}",
        chain_uid.as_str(),
        chain.factory_address.to_lowercase()
    );

    let chain_type = chain.get_chain_type_str();
    let timeout = get_timeout(timeout)?;
    let timeout = env.block.time.plus_seconds(timeout).seconds();

    let event = send_packet_encoded_event(
        &euclid_encoding::repr::to_transport_string(&wire_msg, encoding)
            .map_err(map_encoding_err)?,
        sequence,
        &source_port,
        &destination_port,
        timeout,
        &chain_type,
        PROTOCOL_VERSION,
        encoding.as_u8(),
    );
    Ok(Response::new()
        .add_attribute("action", EUCLID_SEND_PACKET_ENCODED_EVENT)
        .add_event(event))
}

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
    ensure!(
        RELAYER_CONTRACT.load(deps.storage)? == info.sender,
        ContractError::Unauthorized {}
    );
    let chain_uid = ChainUid::create(source_port.split('.').next().unwrap().to_string())?;

    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
    ensure!(
        source_port
            == format!(
                "{chain_uid}.{factory_address}",
                chain_uid = chain_uid.as_str(),
                factory_address = chain.factory_address
            ),
        ContractError::new("Invalid source port")
    );
    ensure!(
        destination_port == format!("vsl.{router}", router = env.contract.address),
        ContractError::new("Invalid destination port")
    );

    let processed_sequence_key =
        CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS.key((chain_uid.clone(), sequence));
    ensure!(
        !processed_sequence_key.has(deps.storage),
        ContractError::Generic {
            err: "Processed sequence already exists".to_string()
        }
    );
    processed_sequence_key.save(deps.storage, &Uint256::from(env.block.height))?;
    let receive_packet_event = receive_packet_event(sequence, &source_port, &destination_port);

    // Strict decode of the declared leg encoding; no fallback probing. The msg
    // arrives as a String in the transport representation; recover the wire
    // bytes first (raw JSON text bytes when Json, 0x hex decode when Abi).
    let encoding = Encoding::from_u8(encoding).map_err(map_encoding_err)?;
    let msg_bytes: Binary = euclid_encoding::repr::from_transport_string(&msg, encoding)
        .map_err(map_encoding_err)?
        .into();
    let received: RouterReceiveMsg =
        transcode::decode_router_receive(&msg_bytes, encoding).map_err(map_encoding_err)?;
    let json = to_json_binary(&received)?;
    let tx_id = received.get_tx_id();

    // Record the receive context for the reply, which transcodes the ack back
    // onto the leg encoding keyed by the send msg's wire tag and emits the
    // single complete acknowledgement event. No acknowledgement event is
    // emitted here; only receive_packet_event fires at receive time.
    IN_FLIGHT_RECEIVE.save(
        deps.storage,
        &InFlightReceive {
            chain_uid: chain_uid.clone(),
            source_port: source_port.clone(),
            destination_port: destination_port.clone(),
            msg: msg_bytes,
            sequence,
            source_chain_type: chain.get_chain_type_str(),
            encoding: encoding.as_u8(),
            wire_tag: received.wire_tag(),
        },
    )?;

    // Internal plumbing stays domain JSON regardless of the wire encoding.
    let internal_msg = ExecuteMsg::ReceivePacketInternalCallback {
        msg: json,
        chain_uid: chain_uid.clone(),
        timeout,
    };
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

pub fn execute_receive_packet_internal_callback(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Binary,
    chain_uid: ChainUid,
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
    let msg: RouterReceiveMsg = from_json(msg)?;
    receive::reusable_internal_call(deps, env, info, msg, chain_uid)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_receive_acknowledgement(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: String,
    sequence: u128,
    source_port: String,
    destination_port: String,
    ack: String,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    ensure!(
        RELAYER_CONTRACT.load(deps.storage)? == info.sender,
        ContractError::Unauthorized {}
    );

    let chain_uid = ChainUid::create(source_port.split('.').next().unwrap().to_string())?;

    ensure!(
        destination_port == format!("vsl.{router}", router = env.contract.address),
        ContractError::new("Invalid destination port")
    );
    let existing_request =
        remove_pending_packet_and_decrement_count(deps.storage, &chain_uid, sequence)?;

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
    let msg: FactoryReceiveMsg = from_json(&existing_request.original_msg)?;

    // The ack rides the wire in the leg encoding recorded at send time;
    // transcode it back to the internal JSON form keyed by the send tag.
    let ack_json = transcode::factory_ack_wire_to_json(msg.wire_tag(), &ack_bytes, encoding)
        .map_err(map_encoding_err)?;

    // Verify chain uid is registerd and is solana chain if its not a register factory msg
    let chain_type = match msg.clone() {
        FactoryReceiveMsg::RegisterFactory(register_msg) => {
            let (chain_type, chain_uid) = (register_msg.chain_type, register_msg.chain_uid);
            ensure!(
                source_port
                    == format!(
                        "{chain_uid}.{factory_address}",
                        chain_uid = chain_uid.as_str(),
                        factory_address = chain_type.factory_address()
                    ),
                ContractError::new("Invalid source port")
            );
            chain_type.tmp_chain_type()?
        }
        _ => {
            let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
            ensure!(
                source_port
                    == format!(
                        "{chain_uid}.{factory_address}",
                        chain_uid = chain_uid.as_str(),
                        factory_address = chain.factory_address
                    ),
                ContractError::new("Invalid source port")
            );
            chain.chain_type.clone()
        }
    };

    let response = ack_and_timeout::reusable_internal_ack_call(
        deps, env, chain_uid, msg, ack_json, chain_type,
    )?;

    let ack_event = receive_acknowledgement_event(sequence, &source_port, &destination_port);
    let response = response.add_event(ack_event);

    Ok(response)
}

pub fn execute_native_receive_callback(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    chain_uid: ChainUid,
    msg: Binary,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    let chain_uid = chain_uid.validate()?.clone();
    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
    // Only native chains can directly use this messages
    ensure!(chain.is_native(), ContractError::Unauthorized {});

    // Only registered factory contract can execute this message
    ensure!(
        chain.factory_address == info.sender.as_str(),
        ContractError::Unauthorized {}
    );
    let msg: RouterReceiveMsg = from_json(msg)?;
    receive::reusable_internal_call(deps, env, info, msg, chain_uid)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_env},
        Addr, Binary, Uint256,
    };
    use euclid::{
        chain::{Chain, ChainType, ChainUid, CosmosChain},
        error::ContractError,
    };

    use crate::{
        contract::execute,
        testing::{
            fixtures::initialized,
            helpers::{MockDeps, TEST_RELAYER},
        },
    };
    use euclid::msgs::router::ExecuteMsg;
    use rstest::*;
    // -----------------------------------------------------------------------
    // Relay handlers: unauthorized callers (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::send_packet(
        Addr::unchecked("external_caller"),
        ExecuteMsg::SendPacket {
            chain: Chain {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                factory_address: "factory1".to_string(),
                chain_type: ChainType::Native {},
            },
            msg: Binary::default(),
            sender: "sender".to_string(),
            timeout: None,
            ack_response: None,
        },
    )]
    #[case::receive_packet(
        Addr::unchecked("attacker"),
        ExecuteMsg::ReceivePacket {
            source_port: "chain1.factory1".to_string(),
            destination_port: "vsl.contract".to_string(),
            msg: String::new(),
            sequence: 0,
            timeout: u64::MAX,
            encoding: 0,
        },
    )]
    #[case::acknowledge_packet(
        Addr::unchecked("attacker"),
        ExecuteMsg::AcknowledgePacket {
            source_port: "chain1.factory1".to_string(),
            destination_port: "vsl.contract".to_string(),
            msg: String::new(),
            sequence: 0,
            ack: String::new(),
        },
    )]
    #[case::receive_packet_internal_callback(
        Addr::unchecked("external"),
        ExecuteMsg::ReceivePacketInternalCallback {
            msg: Binary::default(),
            chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
            timeout: u64::MAX,
        },
    )]
    fn test_relay_handler_rejects_unauthorized_caller(
        mut initialized: MockDeps,
        #[case] sender: Addr,
        #[case] msg: ExecuteMsg,
    ) {
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            msg,
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // ReceivePacket: port & sequence validation (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::invalid_source_port(
        "chain1.wrongfactory",
        false,
        ContractError::new("Invalid source port")
    )]
    #[case::duplicate_sequence(
        "chain1.factory1",
        true,
        ContractError::Generic { err: "Processed sequence already exists".to_string() },
    )]
    fn test_receive_packet_validation_errors(
        mut initialized: MockDeps,
        #[case] source_port: &str,
        #[case] setup_duplicate: bool,
        #[case] expected_error: ContractError,
    ) {
        use crate::testing::helpers::{seed_chain1_native, TEST_RELAYER};

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_chain1_native(&mut initialized);

        if setup_duplicate {
            use crate::relay_state::CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS;
            CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS
                .save(
                    initialized.as_mut().storage,
                    (chain_uid.clone(), 0_u128),
                    &Uint256::from(1_u64),
                )
                .unwrap();
        }

        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: source_port.to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
                msg: String::new(),
                sequence: 0,
                timeout: u64::MAX,
                encoding: 0,
            },
        );
        assert_eq!(res.unwrap_err(), expected_error);
    }

    #[rstest]
    fn test_receive_packet_unregistered_chain_fails(mut initialized: MockDeps) {
        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: "unknownchain.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
                msg: String::new(),
                sequence: 0,
                timeout: u64::MAX,
                encoding: 0,
            },
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // AcknowledgePacket: port validation
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_acknowledge_packet_invalid_destination_port(mut initialized: MockDeps) {
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: "chain1.factory1".to_string(),
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
    // SendPacket: encoded event emission per leg encoding
    // -----------------------------------------------------------------------

    fn release_escrow_msg() -> euclid_ibc::wire::envelope::factory::FactoryReceiveMsg {
        use euclid::token::{Token, TokenType};
        euclid_ibc::wire::envelope::factory::FactoryReceiveMsg::ReleaseEscrow(
            euclid_ibc::wire::msgs::ReleaseEscrowSendMsg {
                sender: euclid::cross_chain_user::CrossChainUser::new(
                    ChainUid::create("chain1".to_string()).unwrap(),
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
                tx_id: "tx-send-1".to_string(),
            },
        )
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

    #[rstest]
    #[case::evm_leg_is_abi(
        ChainType::Evm(euclid::chain::EvmChain { chain_id: "1".to_string() }),
        "evm",
        1
    )]
    #[case::cosmos_leg_is_json(
        ChainType::Cosmos(CosmosChain { chain_id: "cosmos-1".to_string() }),
        "cosmos",
        0
    )]
    fn test_send_packet_emits_encoded_event(
        mut initialized: MockDeps,
        #[case] chain_type: ChainType,
        #[case] expected_chain_type_str: &str,
        #[case] expected_encoding: u8,
    ) {
        use cosmwasm_std::{attr, to_json_binary};
        use euclid::events::EUCLID_SEND_PACKET_ENCODED_EVENT;
        use euclid_encoding::{Encoding, PROTOCOL_VERSION};
        use euclid_ibc::wire::transcode;

        use crate::relay_state::CROSS_CHAIN_PENDING_SEND_PACKETS;

        let env = mock_env();
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let chain = Chain {
            chain_uid: chain_uid.clone(),
            factory_address: "factory1".to_string(),
            chain_type,
        };
        let domain_msg = release_escrow_msg();
        let msg_json = to_json_binary(&domain_msg).unwrap();

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&env.contract.address, &[]),
            ExecuteMsg::SendPacket {
                chain: chain.clone(),
                msg: msg_json.clone(),
                sender: "sender".to_string(),
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

        assert_eq!(
            event_attr(event, "source_port"),
            format!("vsl.{}", env.contract.address.to_string().to_lowercase())
        );
        assert_eq!(event_attr(event, "destination_port"), "chain1.factory1");
        assert_eq!(event_attr(event, "sequence"), "0");
        assert_eq!(
            event_attr(event, "destination_chain_type"),
            expected_chain_type_str
        );
        assert_eq!(event_attr(event, "version"), PROTOCOL_VERSION);
        assert_eq!(event_attr(event, "encoding"), expected_encoding.to_string());

        let msg_attr = event_attr(event, "msg");
        match Encoding::from_u8(expected_encoding).unwrap() {
            // Json legs carry the raw JSON text, byte-identical to the domain JSON.
            Encoding::Json => {
                assert_eq!(msg_attr, String::from_utf8(msg_json.to_vec()).unwrap());
            }
            // Abi legs carry 0x lowercase hex wire bytes that decode back to
            // the domain msg.
            Encoding::Abi => {
                assert!(msg_attr.starts_with("0x"), "msg attr must be 0x hex");
                assert_eq!(msg_attr, msg_attr.to_lowercase());
                let wire = Binary::new(
                    euclid_encoding::repr::from_transport_string(&msg_attr, Encoding::Abi)
                        .expect("msg attr must be 0x hex"),
                );
                let decoded =
                    transcode::decode_factory_receive(&wire, Encoding::Abi).expect("abi decode");
                assert_eq!(decoded, domain_msg);
            }
        }

        // The stored pending packet records the leg encoding; the original msg
        // stays domain JSON for the ack path.
        let pending = CROSS_CHAIN_PENDING_SEND_PACKETS
            .load(initialized.as_ref().storage, (chain_uid, 0_u128))
            .unwrap();
        assert_eq!(pending.encoding, expected_encoding);
        assert_eq!(pending.original_msg, msg_json);

        // The stored wire_msg is exactly the bytes emitted in the send event.
        // Compute the expected bytes independently through the transcode encoder
        // (the router sends FactoryReceiveMsg values) rather than parsing the
        // event attribute back.
        let encoding = Encoding::from_u8(expected_encoding).unwrap();
        let expected_wire = transcode::encode_factory_receive_from_json(&msg_json, encoding)
            .expect("factory receive encode");
        assert_eq!(pending.wire_msg, expected_wire);
        // On the Json leg the wire bytes duplicate the domain JSON.
        if encoding == Encoding::Json {
            assert_eq!(pending.wire_msg, msg_json);
        }
    }

    // -----------------------------------------------------------------------
    // ReceivePacket: no acknowledgement event at receive time + in-flight
    // receive state carrying the full context
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_receive_packet_json_emits_no_ack_event_and_saves_in_flight(mut initialized: MockDeps) {
        use cosmwasm_std::{to_json_binary, CosmosMsg, WasmMsg};
        use euclid::events::{
            EUCLID_RECEIVE_PACKET_EVENT, EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT,
        };

        use crate::relay_state::{InFlightReceive, IN_FLIGHT_RECEIVE};
        use crate::testing::helpers::{register_denom_msg, seed_chain1_native};

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_chain1_native(&mut initialized);

        let domain_msg = register_denom_msg(chain_uid.clone());
        let msg_json = to_json_binary(&domain_msg).unwrap();

        let env = mock_env();
        let destination_port = format!("vsl.{}", env.contract.address);
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: destination_port.clone(),
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
            .any(|e| e.ty == EUCLID_RECEIVE_PACKET_EVENT));

        // In-flight receive context saved for the reply, ports as received.
        let ctx = IN_FLIGHT_RECEIVE
            .load(initialized.as_ref().storage)
            .unwrap();
        assert_eq!(
            ctx,
            InFlightReceive {
                chain_uid,
                source_port: "chain1.factory1".to_string(),
                destination_port,
                msg: msg_json.clone(),
                sequence: 5,
                source_chain_type: "native".to_string(),
                encoding: 0,
                wire_tag: 0, // TAG_REGISTER_DENOM
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

    #[rstest]
    fn test_receive_packet_abi_decodes_and_saves_wire_tag(mut initialized: MockDeps) {
        use cosmwasm_std::to_json_binary;
        use euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT;
        use euclid_encoding::Encoding;

        use crate::relay_state::IN_FLIGHT_RECEIVE;
        use crate::testing::helpers::{register_denom_msg, seed_chain1_native};

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_chain1_native(&mut initialized);

        let domain_msg = register_denom_msg(chain_uid);
        // Typed encode needs no transcode helper; the sample is the wire enum.
        let wire = Binary::from(euclid_encoding::encode(&domain_msg, Encoding::Abi).unwrap());

        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
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

        let ctx = IN_FLIGHT_RECEIVE
            .load(initialized.as_ref().storage)
            .unwrap();
        // The wire bytes are stored as received for the single shot event.
        assert_eq!(ctx.msg, wire);
        assert_eq!(ctx.sequence, 3);
        assert_eq!(ctx.encoding, 1);
        assert_eq!(ctx.wire_tag, 0); // TAG_REGISTER_DENOM

        // Internal plumbing stays domain JSON regardless of the wire encoding.
        let expected_json = to_json_binary(&domain_msg).unwrap();
        let sub = res.messages.first().expect("missing internal callback");
        match &sub.msg {
            cosmwasm_std::CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute { msg, .. }) => {
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
    fn test_receive_packet_strict_decode_errors(
        mut initialized: MockDeps,
        #[case] encoding: u8,
        #[case] msg: String,
    ) {
        use crate::testing::helpers::seed_chain1_native;

        seed_chain1_native(&mut initialized);
        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
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

    #[rstest]
    fn test_acknowledge_packet_abi_leg_rejects_undecodable_ack(mut initialized: MockDeps) {
        use cosmwasm_std::to_json_binary;

        use crate::relay_state::create_pending_packet_and_update_sequence;
        use crate::testing::helpers::seed_chain1_native;

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_chain1_native(&mut initialized);
        let chain = Chain {
            chain_uid: chain_uid.clone(),
            factory_address: "factory1".to_string(),
            chain_type: ChainType::Native {},
        };

        // Pending packet stored with the Abi leg encoding.
        let msg_json = to_json_binary(&release_escrow_msg()).unwrap();
        create_pending_packet_and_update_sequence(
            initialized.as_mut().storage,
            &chain,
            &msg_json,
            None,
            "sender",
            1,
            Binary::default(),
        )
        .unwrap();

        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
                // Valid Abi transport form (0x hex) so the failure comes from
                // the ack transcode, not the representation decode.
                msg: euclid_encoding::repr::to_transport_string(
                    &msg_json,
                    euclid_encoding::Encoding::Abi,
                )
                .unwrap(),
                sequence: 0,
                ack: euclid_encoding::repr::to_transport_string(
                    b"not-abi",
                    euclid_encoding::Encoding::Abi,
                )
                .unwrap(),
            },
        );
        assert!(res.is_err(), "abi leg must reject an undecodable wire ack");
    }

    // -----------------------------------------------------------------------
    // AcknowledgePacket: Amendment B byte check against the stored wire bytes
    // -----------------------------------------------------------------------

    /// A RegisterFactory send msg the router can send to a Cosmos (Json) leg.
    /// The RegisterFactory ack path resolves the chain type from the msg itself
    /// and, on an Error ack for a non-native chain, dispatches with no extra
    /// state, so it round-trips cleanly through the byte check.
    fn register_factory_msg() -> euclid_ibc::wire::envelope::factory::FactoryReceiveMsg {
        use euclid::msgs::router::{RegisterFactoryChainCosmos, RegisterFactoryChainType};
        euclid_ibc::wire::envelope::factory::FactoryReceiveMsg::RegisterFactory(
            euclid_ibc::wire::msgs::RegisterFactorySendMsg {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                chain_type: RegisterFactoryChainType::Cosmos(RegisterFactoryChainCosmos {
                    factory_address: "factory1".to_string(),
                    factory_chain_id: "cosmos-1".to_string(),
                }),
                tx_id: "tx-register-1".to_string(),
            },
        )
    }

    /// Send a RegisterFactory packet on the Cosmos (Json) leg and return the
    /// transport `msg` string emitted in the send event (the raw JSON text).
    fn send_register_factory_and_capture_msg(
        initialized: &mut MockDeps,
        env: &cosmwasm_std::Env,
    ) -> String {
        use cosmwasm_std::to_json_binary;
        use euclid::events::EUCLID_SEND_PACKET_ENCODED_EVENT;

        let chain = Chain {
            chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
            factory_address: "factory1".to_string(),
            chain_type: ChainType::Cosmos(CosmosChain {
                chain_id: "cosmos-1".to_string(),
            }),
        };
        let msg_json = to_json_binary(&register_factory_msg()).unwrap();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&env.contract.address, &[]),
            ExecuteMsg::SendPacket {
                chain,
                msg: msg_json,
                sender: "sender".to_string(),
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

    #[rstest]
    fn test_acknowledge_packet_positive_roundtrip_dispatches(mut initialized: MockDeps) {
        use crate::testing::helpers::seed_chain1_native;

        seed_chain1_native(&mut initialized);
        let env = mock_env();
        let msg = send_register_factory_and_capture_msg(&mut initialized, &env);

        // Exact send msg round-trips: the byte check passes and the ack
        // dispatches (RegisterFactory Error arm on a non-native chain -> Ok).
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
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

    #[rstest]
    fn test_acknowledge_packet_tampered_msg_rejected(mut initialized: MockDeps) {
        use crate::testing::helpers::seed_chain1_native;

        seed_chain1_native(&mut initialized);
        let env = mock_env();
        let msg = send_register_factory_and_capture_msg(&mut initialized, &env);

        // Flip one byte of the transport msg so the recovered wire bytes differ
        // from the stored PendingPacket.wire_msg by exactly one byte.
        let mut tampered = msg.into_bytes();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        let tampered = String::from_utf8(tampered).unwrap();

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
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

    #[rstest]
    fn test_acknowledge_packet_wrong_representation_rejected(mut initialized: MockDeps) {
        use crate::relay_state::create_pending_packet_and_update_sequence;
        use crate::testing::helpers::seed_chain1_native;
        use cosmwasm_std::to_json_binary;

        seed_chain1_native(&mut initialized);
        let chain = Chain {
            chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
            factory_address: "factory1".to_string(),
            chain_type: ChainType::Native {},
        };
        // Pending packet stored with the Abi leg encoding.
        let msg_json = to_json_binary(&release_escrow_msg()).unwrap();
        create_pending_packet_and_update_sequence(
            initialized.as_mut().storage,
            &chain,
            &msg_json,
            None,
            "sender",
            1,
            Binary::default(),
        )
        .unwrap();

        let env = mock_env();
        // Abi leg but the msg is supplied as base64 (no 0x prefix). The
        // representation is only known after the pending packet is loaded, so
        // this fails on the parse-after-load, not before the lookup, and never
        // reaches the byte check.
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
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

    #[rstest]
    fn test_acknowledge_packet_pre_upgrade_empty_wire_msg_rejected(mut initialized: MockDeps) {
        use crate::relay_state::create_pending_packet_and_update_sequence;
        use crate::testing::helpers::seed_chain1_native;
        use cosmwasm_std::to_json_binary;

        seed_chain1_native(&mut initialized);
        let chain = Chain {
            chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
            factory_address: "factory1".to_string(),
            chain_type: ChainType::Native {},
        };
        // Pre-upgrade packet: Json leg with an empty wire_msg (as serde(default)
        // yields for packets stored before Amendment B). A correct ack whose
        // recovered bytes are the real (non-empty) msg still fails the byte
        // check because the stored wire_msg is empty (drain before upgrade).
        let msg_json = to_json_binary(&release_escrow_msg()).unwrap();
        create_pending_packet_and_update_sequence(
            initialized.as_mut().storage,
            &chain,
            &msg_json,
            None,
            "sender",
            0,
            Binary::default(),
        )
        .unwrap();

        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(TEST_RELAYER), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
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

    #[rstest]
    fn test_receive_packet_internal_callback_timed_out(mut initialized: MockDeps) {
        let env = mock_env();
        // timeout=0 is below mock_env block time (1571797419)
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&env.contract.address, &[]),
            ExecuteMsg::ReceivePacketInternalCallback {
                msg: Binary::default(),
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                timeout: 0,
            },
        );
        assert!(matches!(
            res.unwrap_err(),
            ContractError::PacketTimedOut { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // NativeReceiveCallback: access control (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_native_receive_callback_unregistered_chain(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::NativeReceiveCallback {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                msg: Binary::default(),
            },
        );
        assert!(res.is_err());
    }

    #[rstest]
    #[case::non_native_chain(
        ChainType::Cosmos(CosmosChain { chain_id: "cosmos-1".to_string() }),
        "factory1",
        "factory1",
    )]
    #[case::wrong_factory_caller(
        ChainType::Native {},
        "real_factory",
        "attacker",
    )]
    fn test_native_receive_callback_unauthorized(
        mut initialized: MockDeps,
        #[case] chain_type: ChainType,
        #[case] factory_address: &str,
        #[case] caller_name: &str,
    ) {
        use crate::state::CHAIN_UID_TO_CHAIN;

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        CHAIN_UID_TO_CHAIN
            .save(
                initialized.as_mut().storage,
                chain_uid.clone(),
                &Chain {
                    chain_uid: chain_uid.clone(),
                    factory_address: factory_address.to_string(),
                    chain_type,
                },
            )
            .unwrap();

        let caller = initialized.api.addr_make(caller_name);
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&caller, &[]),
            ExecuteMsg::NativeReceiveCallback {
                chain_uid,
                msg: Binary::default(),
            },
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }
}
