use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, MessageInfo, Response,
    StdError, SubMsg, Uint256, WasmMsg,
};
use euclid::{
    chain::{Chain, ChainUid},
    error::ContractError,
    events::{
        receive_acknowledgement_event, receive_packet_event, send_packet_event,
        write_acknowledgement_event, EUCLID_RECEIVE_PACKET_EVENT,
        EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT,
    },
    msgs::router::ExecuteMsg,
    timeout::get_timeout,
};
use euclid_ibc::{
    ack::make_ack_fail, factory_ibc::FactoryCrossChainExecuteMsg,
    router_ibc::RouterCrossChainExecuteMsg,
};

use crate::{
    ibc::{ack_and_timeout, receive},
    relay_state::{
        create_pending_packet_and_update_sequence, remove_pending_packet_and_decrement_count,
        CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS,
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

    let (chain_uid, sequence) = create_pending_packet_and_update_sequence(
        deps.storage,
        &chain,
        &msg,
        ack_response,
        &sender,
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

    let send_packet_event = send_packet_event(
        &source_port,
        &destination_port,
        &msg.to_string(),
        sequence,
        timeout,
        &chain_type,
    );
    Ok(Response::new()
        .add_attribute("action", "euclid-send-packet")
        .add_event(send_packet_event))
}

#[allow(clippy::too_many_arguments)]
pub fn execute_receive_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: Binary,
    sequence: u128,
    source_port: String,
    destination_port: String,
    timeout: u64,
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

    let write_acknowledge_event = write_acknowledgement_event(
        sequence,
        &destination_port,
        &source_port,
        &chain.get_chain_type_str(),
        &msg.to_string(),
    );

    let internal_msg = ExecuteMsg::ReceivePacketInternalCallback {
        msg: msg.clone(),
        chain_uid: chain_uid.clone(),
        timeout,
    };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, CROSS_CHAIN_RECEIVE_REPLY_ID);
    let msg: Result<RouterCrossChainExecuteMsg, StdError> = from_json(&msg);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    Ok(Response::new()
        .add_attribute("method", EUCLID_RECEIVE_PACKET_EVENT)
        .add_attribute("action", EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT)
        .add_attribute("tx_id", tx_id)
        .set_data(make_ack_fail("default_fail".to_string())?)
        .add_event(receive_packet_event)
        .add_event(write_acknowledge_event)
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
    let msg: RouterCrossChainExecuteMsg = from_json(msg)?;
    receive::reusable_internal_call(deps, env, info, msg, chain_uid)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_receive_acknowledgement(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: Binary,
    sequence: u128,
    source_port: String,
    destination_port: String,
    ack: Binary,
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
    remove_pending_packet_and_decrement_count(deps.storage, &chain_uid, sequence)?;

    // TODO: This is lost during relayer encoding and decoding, fix this once relayer is stable
    // ensure!(
    //     existing_request == msg,
    //     ContractError::new("Ack source msg doesn't match with existing request")
    // );

    let msg: FactoryCrossChainExecuteMsg = from_json(msg)?;

    // Verify chain uid is registerd and is solana chain if its not a register factory msg
    let chain_type = match msg.clone() {
        FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_type,
            chain_uid,
            ..
        } => {
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

    let response =
        ack_and_timeout::reusable_internal_ack_call(deps, env, chain_uid, msg, ack, chain_type)?;

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
    let msg: RouterCrossChainExecuteMsg = from_json(msg)?;
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
            msg: Binary::default(),
            sequence: 0,
            timeout: u64::MAX,
        },
    )]
    #[case::acknowledge_packet(
        Addr::unchecked("attacker"),
        ExecuteMsg::AcknowledgePacket {
            source_port: "chain1.factory1".to_string(),
            destination_port: "vsl.contract".to_string(),
            msg: Binary::default(),
            sequence: 0,
            ack: Binary::default(),
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
                msg: Binary::default(),
                sequence: 0,
                timeout: u64::MAX,
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
                msg: Binary::default(),
                sequence: 0,
                timeout: u64::MAX,
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
                msg: Binary::default(),
                sequence: 0,
                ack: Binary::default(),
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Invalid destination port")
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
