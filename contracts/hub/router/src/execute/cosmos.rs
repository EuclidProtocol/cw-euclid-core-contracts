use std::ops::Add;

use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, Event, MessageInfo,
    Response, StdError, SubMsg, Uint128, WasmMsg,
};
use euclid::{chain::ChainUid, error::ContractError, msgs::router::ExecuteMsg};
use euclid_ibc::{
    ack::make_ack_fail,
    msg::{ChainIbcExecuteMsg, HubIbcExecuteMsg},
};

use crate::{
    ibc::{ack_and_timeout, receive},
    reply::COSMOS_RECEIVE_REPLY_ID,
    state::{
        CHAIN_UID_TO_CHAIN, MOCK_RELAYER_ADDRESSES, PACKET_RELAY, PACKET_RELAY_COUNT_CHAIN,
        PROCESSED_PACKET_SEQUENCE,
    },
};

pub fn execute_cosmos_send_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    chain_uid: ChainUid,
    msg: Binary,
) -> Result<Response, ContractError> {
    // Only contract can call this function internally
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    let sequence = PACKET_RELAY_COUNT_CHAIN
        .load(deps.storage, chain_uid.clone())
        .unwrap_or(0);

    PACKET_RELAY.save(deps.storage, (chain_uid.clone(), sequence), &msg)?;

    PACKET_RELAY_COUNT_CHAIN.save(deps.storage, chain_uid.clone(), &sequence.add(1))?;

    let send_packet_event = Event::new("euclid-cosmos-send-packet")
        .add_attribute("msg", msg.to_string())
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("hash", "hash".to_string());

    Ok(Response::new()
        .add_attribute("action", "cosmos-send-packet")
        .add_event(send_packet_event))
}

pub fn execute_cosmos_receive_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    chain_uid: ChainUid,
    msg: Binary,
    sequence: u128,
    hash: String,
) -> Result<Response, ContractError> {
    ensure!(
        MOCK_RELAYER_ADDRESSES
            .load(deps.storage)?
            .contains(&info.sender.to_string()),
        ContractError::Unauthorized {}
    );

    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
    ensure!(chain.is_ibc(), ContractError::Unauthorized {});

    // Only add event if sequence is present, sequence is not present for native calls
    let processed_sequence_key = PROCESSED_PACKET_SEQUENCE.key((chain_uid.clone(), sequence));
    ensure!(
        !processed_sequence_key.has(deps.storage),
        ContractError::Generic {
            err: "Processed sequence already exists".to_string()
        }
    );
    // Save the processed sequence to avoid duplicate events
    processed_sequence_key.save(deps.storage, &Uint128::from(env.block.height))?;
    let receive_packet_event = Event::new("euclid-hub-receive-packet")
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string());

    let write_acknowledge_event = Event::new("euclid-cosmos-write-acknowledgement")
        .add_attribute("msg", msg.to_string())
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("hash", hash.to_string());

    let internal_msg = ExecuteMsg::ReceivePacketInternalCallback {
        msg: msg.clone(),
        chain_uid: chain_uid.clone(),
    };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, COSMOS_RECEIVE_REPLY_ID);
    let msg: Result<ChainIbcExecuteMsg, StdError> = from_json(&msg);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    Ok(Response::new()
        .add_attribute("action", "cosmos-write-acknowledgement")
        .add_attribute("method", "cosmos_packet_receive")
        .add_attribute("tx_id", tx_id)
        .set_data(make_ack_fail("default_fail".to_string())?)
        .add_event(write_acknowledge_event)
        .add_event(receive_packet_event)
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
    let msg: ChainIbcExecuteMsg = from_json(msg)?;
    receive::reusable_internal_call(deps, env, info, msg, chain_uid)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_receive_acknowledgement(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    chain_uid: ChainUid,
    msg: Binary,
    sequence: u128,
    _hash: String,
    ack: Binary,
) -> Result<Response, ContractError> {
    ensure!(
        MOCK_RELAYER_ADDRESSES
            .load(deps.storage)?
            .contains(&info.sender.to_string()),
        ContractError::Unauthorized {}
    );
    let _existing_request = PACKET_RELAY.load(deps.storage, (chain_uid.clone(), sequence))?;
    // Remove the existing request as its already relayed now
    PACKET_RELAY.remove(deps.storage, (chain_uid.clone(), sequence));

    let maybe_chain_type = CHAIN_UID_TO_CHAIN.may_load(deps.storage, chain_uid.clone())?;

    let msg: HubIbcExecuteMsg = from_json(msg)?;

    // Verify chain uid is registerd and is cosmos chain if its not a register factory msg
    match msg {
        HubIbcExecuteMsg::RegisterFactory { .. } => {}
        _ => {
            ensure!(
                maybe_chain_type.is_some() && maybe_chain_type.unwrap().is_ibc(),
                ContractError::Unauthorized {}
            );
        }
    }

    let response = ack_and_timeout::reusable_internal_ack_call(deps, env, msg, ack)?;

    let response = response.add_event(
        Event::new("euclid-hub-receive-acknowledgement")
            .add_attribute("chain_uid", chain_uid.to_string())
            .add_attribute("sequence", sequence.to_string()),
    );

    Ok(response)
}
