use cosmwasm_std::{
    to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, Response, Uint128,
};
use euclid::{
    error::ContractError,
    pool::{Pool, PoolRequest},
    token::{PairInfo, Token},
};
use euclid_ibc::msg::IbcExecuteMsg;

use crate::state::{POOL_REQUESTS, STATE};

// Function to send IBC request to Router in VLS to create a new pool
pub fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pair_info: PairInfo,
    channel: String,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    let mut msgs: Vec<CosmosMsg> = Vec::new();

    // Create pool request id
    let pool_rq_id = format!(
        "{}-{}-{}",
        info.sender.clone().to_string(),
        env.block.height.clone(),
        env.block.time.to_string()
    );

    // Create a Request in state
    let pool_request = PoolRequest {
        chain: state.chain_id.clone(),
        pool_rq_id: pool_rq_id.clone(),
        channel: channel.clone(),
    };
    POOL_REQUESTS.save(deps.storage, info.sender.clone().to_string(), &pool_request)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::RequestPoolCreation {
            pool_rq_id,
            chain: state.chain_id,
            pair_info,
        })
        .unwrap(),

        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
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
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    let pool_address = info.sender;

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
        })
        .unwrap(),
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
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
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    let pool_address = info.sender.clone();

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&IbcExecuteMsg::AddLiquidity {
            chain_id: state.chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id: format!(
                "{}-{}-{}",
                pool_address.to_string(),
                env.block.height.clone(),
                env.block.time.to_string()
            ),
            pool_address: pool_address.clone().to_string(),
        })
        .unwrap(),
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_message(msg))
}
