use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, Response,
    Uint128,
};
use euclid::{
    error::ContractError,
    timeout::get_timeout,
    token::{PairInfo, Token},
};
use euclid_ibc::msg::ChainIbcExecuteMsg;

use crate::state::{generate_pool_req, STATE};

// Function to send IBC request to Router in VLS to create a new pool
pub fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pair_info: PairInfo,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        state.hub_channel.is_some(),
        ContractError::Generic {
            err: "Hub Channel doesn't exist".to_string()
        }
    );
    let channel = state.hub_channel.unwrap();

    // Create a Request in state
    let pool_request = generate_pool_req(deps, &info.sender, env.block.chain_id, channel.clone())?;

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::RequestPoolCreation {
            pool_rq_id: pool_request.pool_rq_id,
            pair_info,
        })?,

        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    Ok(Response::new()
        .add_attribute("method", "request_pool_creation")
        .add_message(ibc_packet))
}

// Function to send IBC request to Router in VLS to perform a swap
pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: Token,
    asset_amount: Uint128,
    min_amount_out: Uint128,
    swap_id: String,
    timeout: Option<u64>,
    vlp_address: String,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    ensure!(
        state.hub_channel.is_some(),
        ContractError::Generic {
            err: "Hub Channel doesn't exist".to_string()
        }
    );
    let channel = state.hub_channel.unwrap();

    let pool_address = info.sender;

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::Swap {
            to_chain_id: state.chain_id,
            asset_in: asset,
            amount_in: asset_amount,
            min_amount_out,
            channel,
            swap_id,
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
    liquidity_id: String,
    timeout: Option<u64>,
    vlp_address: String,
) -> Result<Response, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;

    ensure!(
        state.hub_channel.is_some(),
        ContractError::Generic {
            err: "Hub Channel doesn't exist".to_string()
        }
    );
    let channel = state.hub_channel.unwrap();

    let pool_address = info.sender.clone();

    let timeout = get_timeout(timeout)?;

    // Create IBC packet to send to Router
    let ibc_packet = IbcMsg::SendPacket {
        channel_id: channel.clone(),
        data: to_json_binary(&ChainIbcExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id,
            pool_address: pool_address.clone().to_string(),
            vlp_address,
        })?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
    };

    let msg = CosmosMsg::Ibc(ibc_packet);

    Ok(Response::new()
        .add_attribute("method", "add_liquidity_request")
        .add_message(msg))
}

// Function to update the pool code ID
pub fn execute_update_pool_code_id(
    deps: DepsMut,
    info: MessageInfo,
    new_pool_code_id: u64,
) -> Result<Response, ContractError> {
    // Load the state
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        // Ensure that only the admin can update the pool code ID
        ensure!(info.sender == state.admin, ContractError::Unauthorized {});

        // Update the pool code ID
        state.pool_code_id = new_pool_code_id;
        Ok(state)
    })?;

    Ok(Response::new()
        .add_attribute("method", "update_pool_code_id")
        .add_attribute("new_pool_code_id", new_pool_code_id.to_string()))
}
