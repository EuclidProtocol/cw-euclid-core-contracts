use cosmwasm_std::{
    to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, Response, Uint128,
};
use euclid::{
    error::ContractError,
    timeout::get_timeout,
    token::{PairInfo, Token},
};
use euclid_ibc::msg::IbcExecuteMsg;

use crate::state::{generate_pool_req, STATE};

// Function to send IBC request to Router in VLS to create a new pool
pub fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pair_info: PairInfo,
    channel: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    let mut msgs: Vec<CosmosMsg> = Vec::new();

    // Create a Request in state
    let pool_request = generate_pool_req(deps, &info.sender, env.block.chain_id, channel.clone())?;

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::RequestPoolCreation {
            pool_rq_id: pool_request.pool_rq_id,
            chain: state.chain_id,
            pair_info,
        })?,

        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    msgs.push(ibc_packet.into());

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_messages(msgs))
}

// Function to send IBC request to Router in VLS to perform a swap
pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: Token,
    asset_amount: Uint128,
    min_amount_out: Uint128,
    channel: String,
    swap_id: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    let pool_address = info.sender;

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::Swap {
            chain_id: state.chain_id,
            asset,
            asset_amount,
            min_amount_out,
            channel,
            swap_id,
            pool_address,
        })?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_message(msg))
}

// Function to send IBC request to Router in VLS to add liquidity to a pool
pub fn execute_add_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    channel: String,
    liquidity_id: String,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    let pool_address = info.sender.clone();

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::AddLiquidity {
            chain_id: state.chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id,
            pool_address: pool_address.clone().to_string(),
        })?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "add_liquidity_request")
        .add_message(msg))
}
