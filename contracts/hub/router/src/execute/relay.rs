use std::ops::Add;

use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, MessageInfo, Response,
    StdError, SubMsg, Uint128, WasmMsg,
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
    router_ibc::RouterCrossChainExecuteMsg, state::PendingPacket,
};

use crate::{
    ibc::{ack_and_timeout, receive},
    relay_state::{
        CROSS_CHAIN_LATEST_SEQUENCE_COUNT, CROSS_CHAIN_PENDING_PACKET_SENDER,
        CROSS_CHAIN_PENDING_SEND_PACKETS, CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS,
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
    // Only contract can call this function internally
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    let sequence = CROSS_CHAIN_LATEST_SEQUENCE_COUNT
        .load(deps.storage, chain.chain_uid.clone())
        .unwrap_or(0);

    CROSS_CHAIN_PENDING_SEND_PACKETS.save(
        deps.storage,
        (chain.chain_uid.clone(), sequence),
        &PendingPacket {
            chain_uid: chain.chain_uid.clone(),
            original_msg: msg.clone(),
            ack_response,
        },
    )?;
    CROSS_CHAIN_PENDING_PACKET_SENDER.save(
        deps.storage,
        (chain.chain_uid.clone(), sequence),
        &sender,
    )?;
    CROSS_CHAIN_LATEST_SEQUENCE_COUNT.save(
        deps.storage,
        chain.chain_uid.clone(),
        &sequence.add(1),
    )?;

    let source_port = format!("vsl.{}", env.contract.address.to_string().to_lowercase());
    let destination_port = format!(
        "{}.{}",
        chain.chain_uid.as_str(),
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
    _timeout: Option<u64>,
) -> Result<Response, ContractError> {
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
    processed_sequence_key.save(deps.storage, &Uint128::from(env.block.height))?;
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
) -> Result<Response, ContractError> {
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
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
    ensure!(
        RELAYER_CONTRACT.load(deps.storage)? == info.sender,
        ContractError::Unauthorized {}
    );

    let chain_uid = ChainUid::create(source_port.split('.').next().unwrap().to_string())?;

    ensure!(
        destination_port == format!("vsl.{router}", router = env.contract.address),
        ContractError::new("Invalid destination port")
    );
    let _existing_request =
        CROSS_CHAIN_PENDING_SEND_PACKETS.load(deps.storage, (chain_uid.clone(), sequence))?;
    let _sender =
        CROSS_CHAIN_PENDING_PACKET_SENDER.load(deps.storage, (chain_uid.clone(), sequence))?;

    // TODO: This is lost during relayer encoding and decoding, fix this once relayer is stable
    // ensure!(
    //     existing_request == msg,
    //     ContractError::new("Ack source msg doesn't match with existing request")
    // );

    // Remove the existing request as its already relayed now
    CROSS_CHAIN_PENDING_SEND_PACKETS.remove(deps.storage, (chain_uid.clone(), sequence));
    CROSS_CHAIN_PENDING_PACKET_SENDER.remove(deps.storage, (chain_uid.clone(), sequence));

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
