#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult};
use cw2::set_contract_version;
use euclid::error::ContractError;
// use cw2::set_contract_version;

use crate::query::{query_connection_count, query_pool_address, query_state, query_timeout_count};
use crate::reply::INSTANTIATE_REPLY_ID;
use crate::state::{State, STATE};
use crate::{execute, query, reply};
use euclid::msgs::factory::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:factory";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        router_contract: msg.router_contract.clone(),
        chain_id: msg.chain_id.clone(),
        admin: info.sender.clone().to_string(),
        pool_code_id: msg.pool_code_id.clone(),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", msg.router_contract)
        .add_attribute("chain_id", msg.chain_id.clone()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RequestPoolCreation { pair_info, channel } => {
            execute::execute_request_pool_creation(deps, env, info, pair_info, channel)
        }
        ExecuteMsg::ExecuteSwap {
            asset,
            asset_amount,
            min_amount_out,
            channel,
            swap_id,
        } => execute::execute_swap(
            deps,
            env,
            info,
            asset,
            asset_amount,
            min_amount_out,
            channel,
            swap_id,
        ),
        ExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            channel,
        } => execute::execute_add_liquidity(
            deps,
            env,
            info,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            channel,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetPool { vlp } => query::get_pool(deps, vlp),
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetConnectionCount { channel_id } => query_connection_count(deps, channel_id),
        QueryMsg::GetTimeoutCount { channel_id } => query_timeout_count(deps, channel_id),
        QueryMsg::GetPoolAddress { vlp_address } => query_pool_address(deps, vlp_address),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_REPLY_ID => reply::on_pool_instantiate_reply(deps, msg),
        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}

#[cfg(test)]
mod tests {}
