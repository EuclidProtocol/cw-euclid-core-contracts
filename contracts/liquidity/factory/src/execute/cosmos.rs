use std::ops::Add;

use cosmwasm_std::{
    coins, ensure, from_json, to_json_binary, BankMsg, Binary, CosmosMsg, DepsMut, Env, Event,
    MessageInfo, Response, StdError, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::{CrossChainUser, IbcChain},
    error::ContractError,
    msgs::factory::{usage_fee::calc_fee, ExecuteMsg},
    utils::fund_manager::FundManager,
};
use euclid_ibc::{
    ack::make_ack_fail,
    msg::{ChainIbcExecuteMsg, HubIbcExecuteMsg},
};

use crate::{
    ibc::{ack_and_timeout, receive},
    reply::COSMOS_RECEIVE_REPLY_ID,
    state::{
        COSMOS_PACKET_RELAY_MAP, COSMOS_PACKET_RELAY_SEQUENCE_COUNT, CUSTOM_LIMITS,
        GLOBAL_LIMIT_FOR_USERS, MOCK_RELAYER_ADDRESS, PACKET_RELAY_SEQUENCE_COUNT_LIMIT,
        PENDING_PACKETS, PROCESSED_PACKET_SEQUENCE, RELAY_COUNT_USER, STATE,
    },
};

const DEFAULT_GLOBAL_LIMIT_FOR_USERS: u128 = 10;
const DEFAULT_GLOBAL_LIMIT_FOR_CHAINS: u128 = 1000;

/**
 * Always run by contract itself to trigger send packet event and also increment sequence count
 */
pub fn execute_send_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    cross_chain_user: CrossChainUser,
    msg: Binary,
    funds_manager: &mut FundManager,
) -> Result<Response, ContractError> {
    // Only contract can call this function internally
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );

    // Check if the sender has a custom limit
    let custom_limit = CUSTOM_LIMITS
        .may_load(deps.storage, cross_chain_user.address.clone())
        .unwrap_or(None);

    let user_relay_count = RELAY_COUNT_USER
        .load(deps.storage, cross_chain_user.address.clone())
        .unwrap_or(0);
    let new_user_relay_count = user_relay_count.add(1);

    let limit = custom_limit.unwrap_or(
        GLOBAL_LIMIT_FOR_USERS
            .load(deps.storage)
            .unwrap_or(DEFAULT_GLOBAL_LIMIT_FOR_USERS),
    );
    let mut response = Response::new();
    if new_user_relay_count.gt(&limit) {
        let config = STATE.load(deps.storage)?.usage_fee_config;
        let fee = calc_fee(&config, new_user_relay_count);
        let (denom, _amount) = funds_manager.get_single_fund()?;

        funds_manager.use_fund(fee, &denom)?;
        let cosmos_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: config.fee_recipient,
            amount: coins(fee.u128(), &denom),
        });
        response = response.add_message(cosmos_msg);
    }

    let sequence_limit = PACKET_RELAY_SEQUENCE_COUNT_LIMIT
        .load(deps.storage)
        .unwrap_or(DEFAULT_GLOBAL_LIMIT_FOR_CHAINS);

    let pending_packets = PENDING_PACKETS.load(deps.storage).unwrap_or(0);

    ensure!(
        pending_packets.lt(&sequence_limit),
        ContractError::Generic {
            err: "Sequence limit exceeded".to_string()
        }
    );

    let sequence = COSMOS_PACKET_RELAY_SEQUENCE_COUNT
        .load(deps.storage)
        .unwrap_or(0);
    COSMOS_PACKET_RELAY_MAP.save(deps.storage, sequence, &msg)?;

    // Update counts
    COSMOS_PACKET_RELAY_SEQUENCE_COUNT.save(deps.storage, &sequence.add(1))?;
    PENDING_PACKETS.save(deps.storage, &pending_packets.add(1))?;
    RELAY_COUNT_USER.save(
        deps.storage,
        cross_chain_user.address.clone(),
        &new_user_relay_count,
    )?;

    let send_packet_event = Event::new("euclid-send-packet")
        .add_attribute("msg", msg.to_string())
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("hash", "hash".to_string());

    Ok(response
        .add_attribute("action", "send-packet")
        .add_event(send_packet_event))
}

// Always run by relayer to trigger a receive packeg event. At the end of receive, there will be a write acknowledgement event
pub fn execute_cosmos_receive_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: Binary,
    sequence: u128,
    hash: String,
) -> Result<Response, ContractError> {
    ensure!(
        info.sender.to_string() == MOCK_RELAYER_ADDRESS.load(deps.storage)?,
        ContractError::Unauthorized {}
    );

    let processed_sequence_key = PROCESSED_PACKET_SEQUENCE.key(sequence);
    ensure!(
        !processed_sequence_key.has(deps.storage),
        ContractError::Generic {
            err: "Processed sequence already exists".to_string()
        }
    );
    // Save the processed sequence to avoid duplicate events
    processed_sequence_key.save(deps.storage, &Uint128::from(env.block.height))?;

    let receive_packet_event =
        Event::new("euclid-receive-packet").add_attribute("sequence", sequence.to_string());

    let write_acknowledge_event = Event::new("euclid-write-acknowledgement")
        .add_attribute("msg", msg.to_string())
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("hash", hash.to_string());

    let internal_msg = ExecuteMsg::CosmosReceivePacketInternalCallback { msg: msg.clone() };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, COSMOS_RECEIVE_REPLY_ID);
    let msg: Result<HubIbcExecuteMsg, StdError> = from_json(&msg);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    Ok(Response::new()
        .add_attribute("action", "write-acknowledgement")
        .add_attribute("method", "packet_receive")
        .add_attribute("tx_id", tx_id)
        .set_data(make_ack_fail("default_fail".to_string())?)
        .add_event(receive_packet_event)
        .add_event(write_acknowledge_event)
        .add_submessage(sub_msg))
}

// Always run by contract itself to trigger a receive packet event. This is needed because we should never fail receive packet event
pub fn execute_cosmos_receive_packet_internal_callback(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: Binary,
) -> Result<Response, ContractError> {
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );
    let msg: HubIbcExecuteMsg = from_json(msg)?;
    receive::reusable_internal_call(deps, env, msg)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_cosmos_receive_acknowledgement(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    msg: Binary,
    sequence: u128,
    _hash: String,
    ack: Binary,
) -> Result<Response, ContractError> {
    ensure!(
        info.sender.to_string() == MOCK_RELAYER_ADDRESS.load(deps.storage)?,
        ContractError::Unauthorized {}
    );
    let _existing_request = COSMOS_PACKET_RELAY_MAP.load(deps.storage, sequence)?;

    // TODO: This is lost during relayer encoding and decoding, fix this once relayer is stable
    // ensure!(
    //     existing_request == msg,
    //     ContractError::new("Ack source msg doesn't match with existing request")
    // );

    // Remove the existing request as its already relayed now
    COSMOS_PACKET_RELAY_MAP.remove(deps.storage, sequence);

    let _chain_type = euclid::chain::ChainType::Ibc(IbcChain {
        from_hub_channel: "".to_string(),
        from_factory_channel: "".to_string(),
    });

    let msg: ChainIbcExecuteMsg = from_json(msg)?;
    let state = STATE.load(deps.storage)?;

    PENDING_PACKETS.update(deps.storage, |total| {
        Ok::<u128, StdError>(total.checked_sub(1).unwrap_or(0))
    })?;

    let response =
        ack_and_timeout::reusable_internal_ack_call(deps, env, msg, ack, state.is_native)?;
    let ack_event = Event::new("euclid-receive-acknowledgement")
        .add_attribute("sequence", sequence.to_string());
    let response = response.add_event(ack_event);

    Ok(response)
}
