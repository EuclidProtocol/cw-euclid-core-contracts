use std::ops::Add;

use cosmwasm_std::{
    ensure, from_json, to_json_binary, Addr, Binary, CosmosMsg, DepsMut, Env, MessageInfo,
    Response, StdError, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::ChainUid,
    error::ContractError,
    events::{
        receive_acknowledgement_event, receive_packet_event, send_packet_event,
        write_acknowledgement_event, EUCLID_RECEIVE_PACKET_EVENT,
        EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT,
    },
    msgs::{factory::ExecuteMsg, hook::EuclidAcknowledgement},
    timeout::get_timeout,
};
use euclid_ibc::{
    ack::make_ack_fail, factory_ibc::FactoryCrossChainExecuteMsg,
    router_ibc::RouterCrossChainExecuteMsg, state::PendingPacket,
};

use crate::{
    ibc::{ack_and_timeout, receive},
    rate_limit::{
        ensure_rate_limit_exceeded, USER_PENDING_PACKETS_COUNT, USER_TOTAL_PACKETS_COUNT,
    },
    relay_state::{
        CROSS_CHAIN_LATEST_SEQUENCE_COUNT, CROSS_CHAIN_PENDING_PACKETS_COUNT,
        CROSS_CHAIN_PENDING_PACKET_SENDER, CROSS_CHAIN_PENDING_SEND_PACKETS,
        CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS,
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
    // Only contract can call this function internally
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    let response = Response::new();

    let factory_state = STATE.load(deps.storage)?;

    ensure_rate_limit_exceeded(&deps, sender.clone())?;

    let sequence = CROSS_CHAIN_LATEST_SEQUENCE_COUNT
        .load(deps.storage)
        .unwrap_or(0);

    // Make sure the sequence is not already used, this is just an extra check to avoid duplicate sequence which can cause issues in receive packet event
    ensure!(
        !CROSS_CHAIN_PENDING_SEND_PACKETS.has(deps.storage, sequence),
        ContractError::Generic {
            err: "Sequence already exists".to_string()
        }
    );

    CROSS_CHAIN_PENDING_SEND_PACKETS.save(
        deps.storage,
        sequence,
        &PendingPacket {
            chain_uid: ChainUid::vsl_chain_uid()?,
            original_msg: msg.clone(),
            ack_response,
        },
    )?;
    CROSS_CHAIN_PENDING_PACKET_SENDER.save(deps.storage, sequence, &sender)?;
    CROSS_CHAIN_LATEST_SEQUENCE_COUNT.save(deps.storage, &sequence.add(1))?;
    let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
        .may_load(deps.storage)?
        .unwrap_or(0);

    CROSS_CHAIN_PENDING_PACKETS_COUNT.save(
        deps.storage,
        &count.checked_add(1).ok_or(ContractError::new("Overflow"))?,
    )?;

    let user_pending_packets_count = USER_PENDING_PACKETS_COUNT
        .load(deps.storage, sender.clone())
        .unwrap_or(0);

    // Update user pending packets count
    USER_PENDING_PACKETS_COUNT.save(
        deps.storage,
        sender.clone(),
        &user_pending_packets_count
            .checked_add(1)
            .ok_or(ContractError::new("Overflow"))?,
    )?;
    USER_TOTAL_PACKETS_COUNT.update(
        deps.storage,
        sender.clone(),
        |count| -> Result<_, ContractError> {
            count
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(ContractError::new("Overflow"))
        },
    )?;

    let source_port = format!("{}.{}", *factory_state.chain_uid, env.contract.address);

    let destination_port = format!("vsl.{}", factory_state.router_contract);

    let timeout = get_timeout(timeout)?;
    let timeout = env.block.time.plus_seconds(timeout).seconds();

    let send_packet_event = send_packet_event(
        &source_port,
        &destination_port,
        &msg.to_string(),
        sequence,
        timeout,
        "cosmos",
    );

    Ok(response
        .add_attribute("action", "euclid-send-packet")
        .add_event(send_packet_event))
}

// Always run by relayer to trigger a receive packeg event. At the end of receive, there will be a write acknowledgement event
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
    processed_sequence_key.save(deps.storage, &Uint128::from(env.block.height))?;

    let receive_packet_event = receive_packet_event(sequence, &source_port, &destination_port);

    let write_acknowledge_event = write_acknowledgement_event(
        sequence,
        &destination_port,
        &source_port,
        "cosmos",
        &msg.to_string(),
    );

    let internal_msg = ExecuteMsg::ReceivePacketInternalCallback {
        msg: msg.clone(),
        timeout,
    };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, CROSS_CHAIN_RECEIVE_REPLY_ID);
    let msg: Result<FactoryCrossChainExecuteMsg, StdError> = from_json(&msg);
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

// Always run by contract itself to trigger a receive packet event. This is needed because we should never fail receive packet event
pub fn execute_receive_packet_internal_callback(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Binary,
    timeout: u64,
) -> Result<Response, ContractError> {
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
    let msg: FactoryCrossChainExecuteMsg = from_json(msg)?;
    receive::reusable_internal_call(deps, env, msg)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_receive_acknowledgement(
    deps: &mut DepsMut,
    info: MessageInfo,
    env: Env,
    msg: Binary,
    sequence: u128,
    source_port: String,
    destination_port: String,
    ack: Binary,
) -> Result<Response, ContractError> {
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
    let existing_request = CROSS_CHAIN_PENDING_SEND_PACKETS.load(deps.storage, sequence)?;
    let sender = CROSS_CHAIN_PENDING_PACKET_SENDER.load(deps.storage, sequence)?;

    // TODO: This is lost during relayer encoding and decoding, fix this once relayer is stable
    // ensure!(
    //     existing_request == msg,
    //     ContractError::new("Ack source msg doesn't match with existing request")
    // );

    // Remove the existing request as its already relayed now
    CROSS_CHAIN_PENDING_SEND_PACKETS.remove(deps.storage, sequence);
    CROSS_CHAIN_PENDING_PACKET_SENDER.remove(deps.storage, sequence);
    USER_PENDING_PACKETS_COUNT.update(
        deps.storage,
        sender.clone(),
        |count| -> Result<_, ContractError> {
            count
                .unwrap_or(0)
                .checked_sub(1)
                .ok_or(ContractError::new("Overflow"))
        },
    )?;

    let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
        .may_load(deps.storage)?
        // Getting to a point where the item wasn't loaded shouldn't be possible, but just in case, we're defaulting to 1 to avoid overflow error in the next operation
        .unwrap_or(1);

    CROSS_CHAIN_PENDING_PACKETS_COUNT.save(
        deps.storage,
        &count.checked_sub(1).ok_or(ContractError::new("Overflow"))?,
    )?;

    let msg: RouterCrossChainExecuteMsg = from_json(msg)?;

    let response =
        ack_and_timeout::reusable_internal_ack_call(deps, env, msg, ack.clone(), state.is_native)?;
    let ack_event = receive_acknowledgement_event(sequence, &source_port, &destination_port);
    let mut response = response.add_event(ack_event);

    if let Some(ack_response) = existing_request.ack_response {
        let is_contract = deps
            .querier
            .query_wasm_contract_info(sender.to_string())
            .is_ok();
        if is_contract {
            let ack_hook_msg = EuclidAcknowledgement {
                ack,
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
    let msg: FactoryCrossChainExecuteMsg = from_json(msg)?;
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
