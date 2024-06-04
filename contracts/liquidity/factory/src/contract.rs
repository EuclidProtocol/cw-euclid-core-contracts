#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::error::ContractError;
// use cw2::set_contract_version;

use crate::query::{query_all_pools, query_state};
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
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        router_contract: msg.router_contract.clone(),
        chain_id: env.block.chain_id,
        admin: info.sender.clone().to_string(),
        pool_code_id: msg.pool_code_id,
        hub_channel: None,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", msg.router_contract)
        .add_attribute("chain_id", state.chain_id))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RequestPoolCreation { pair_info, timeout } => {
            execute::execute_request_pool_creation(deps, env, info, pair_info, timeout)
        }
        ExecuteMsg::ExecuteSwap {
            asset,
            asset_amount,
            min_amount_out,
            swap_id,
            timeout,
            vlp_address,
        } => execute::execute_swap(
            deps,
            env,
            info,
            asset,
            asset_amount,
            min_amount_out,
            swap_id,
            timeout,
            vlp_address,
        ),
        ExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id,
            timeout,
            vlp_address,
        } => execute::execute_add_liquidity(
            deps,
            env,
            info,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id,
            timeout,
            vlp_address,
        ),
        ExecuteMsg::UpdatePoolCodeId { new_pool_code_id } => {
            execute::execute_update_pool_code_id(deps, info, new_pool_code_id)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetPool { vlp } => query::get_pool(deps, vlp),
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
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
