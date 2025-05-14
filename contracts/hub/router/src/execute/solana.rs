use std::ops::Add;

use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, Event, MessageInfo,
    Response, StdError, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::{ChainUid, SolanaChain},
    error::ContractError,
    msgs::router::ExecuteMsg,
};
use euclid_ibc::{
    ack::make_ack_fail,
    msg::{ChainIbcExecuteMsg, HubIbcExecuteMsg},
};

use crate::{
    ibc::{ack_and_timeout, receive},
    reply::SOLANA_RECEIVE_REPLY_ID,
    state::{
        CHAIN_UID_TO_CHAIN, MOCK_RELAYER_ADDRESSES, PROCESSED_PACKET_SEQUENCE,
        SOLANA_PACKET_RELAY_MAP, SOLANA_PACKET_RELAY_SEQUENCE_COUNT,
    },
};

pub fn execute_solana_send_packet(
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

    let sequence = SOLANA_PACKET_RELAY_SEQUENCE_COUNT
        .load(deps.storage, chain_uid.clone())
        .unwrap_or(0);

    SOLANA_PACKET_RELAY_MAP.save(deps.storage, (chain_uid.clone(), sequence), &msg)?;

    SOLANA_PACKET_RELAY_SEQUENCE_COUNT.save(deps.storage, chain_uid.clone(), &sequence.add(1))?;

    let send_packet_event = Event::new("euclid-solana-send-packet")
        .add_attribute("msg", msg.to_string())
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("hash", "hash".to_string());

    Ok(Response::new()
        .add_attribute("action", "solana-send-packet")
        .add_event(send_packet_event))
}

pub fn execute_solana_receive_packet(
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
    ensure!(chain.is_solana(), ContractError::Unauthorized {});

    let processed_sequence_key = PROCESSED_PACKET_SEQUENCE.key((chain_uid.clone(), sequence));
    ensure!(
        !processed_sequence_key.has(deps.storage),
        ContractError::Generic {
            err: "Processed sequence already exists".to_string()
        }
    );
    processed_sequence_key.save(deps.storage, &Uint128::from(env.block.height))?;
    let receive_packet_event = Event::new("euclid-hub-receive-packet")
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string());

    let write_acknowledge_event = Event::new("euclid-solana-write-acknowledgement")
        .add_attribute("msg", msg.to_string())
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("hash", hash.to_string());

    let internal_msg = ExecuteMsg::SolanaReceivePacketInternalCallback {
        msg: msg.clone(),
        chain_uid: chain_uid.clone(),
    };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, SOLANA_RECEIVE_REPLY_ID);
    let msg: Result<ChainIbcExecuteMsg, StdError> = from_json(&msg);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    Ok(Response::new()
        .add_attribute("action", "solana-write-acknowledgement")
        .add_attribute("method", "solana_packet_receive")
        .add_attribute("tx_id", tx_id)
        .set_data(make_ack_fail("default_fail".to_string())?)
        .add_event(write_acknowledge_event)
        .add_event(receive_packet_event)
        .add_submessage(sub_msg))
}

pub fn execute_solana_receive_packet_internal_callback(
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
pub fn execute_solana_receive_acknowledgement(
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
    let _existing_request =
        SOLANA_PACKET_RELAY_MAP.load(deps.storage, (chain_uid.clone(), sequence))?;

    // TODO: This is lost during relayer encoding and decoding, fix this once relayer is stable
    // ensure!(
    //     existing_request == msg,
    //     ContractError::new("Ack source msg doesn't match with existing request")
    // );

    // Remove the existing request as its already relayed now

    SOLANA_PACKET_RELAY_MAP.remove(deps.storage, (chain_uid.clone(), sequence));

    let chain_type = euclid::chain::ChainType::Solana(SolanaChain {});

    let msg: HubIbcExecuteMsg = from_json(msg)?;

    // Verify chain uid is registerd and is solana chain if its not a register factory msg
    match msg {
        HubIbcExecuteMsg::RegisterFactory { .. } => {}
        _ => {
            let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
            ensure!(chain.is_solana(), ContractError::Unauthorized {});
        }
    }

    let response = ack_and_timeout::reusable_internal_ack_call(deps, env, msg, ack, chain_type)?;
    let response = response.add_event(
        Event::new("euclid-hub-receive-acknowledgement")
            .add_attribute("chain_uid", chain_uid.to_string())
            .add_attribute("sequence", sequence.to_string()),
    );
    Ok(response)
}
