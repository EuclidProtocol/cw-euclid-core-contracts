#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::error::ContractError;
use euclid::msgs::pool::CallbackExecuteMsg;
// use cw2::set_contract_version;

use crate::execute::{
    add_liquidity_request, execute_complete_add_liquidity, execute_complete_swap,
    execute_reject_add_liquidity, execute_reject_swap, execute_swap_request, receive_cw20,
};
use crate::query::{
    get_pool, get_vlp, pair_info, pending_liquidity, pending_swaps, pool_reserves, query_all_pools,
    query_state,
};
use crate::reply;
use crate::reply::INSTANTIATE_REPLY_ID;
use crate::state::{FACTORY_ADDRESS, TOKEN_ID};
use euclid::msgs::escrow::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:escrow";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    TOKEN_ID.save(deps.storage, &msg.token_id)?;
    // Set the sender as the factory address, since we want the factory to instantiate the escrow.
    FACTORY_ADDRESS.save(deps.storage, &info.sender)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("token_id", msg.token_id.id))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateAllowedDenoms { denoms } => {
            execute_update_allowed_denoms(deps, env, info)
        }
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetPool { vlp } => get_pool(deps, vlp),
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
        // Pool Queries //
        QueryMsg::PairInfo {} => pair_info(deps),
        QueryMsg::GetVlp {} => get_vlp(deps),
        QueryMsg::PendingSwapsUser {
            user,
            upper_limit,
            lower_limit,
        } => pending_swaps(deps, user, lower_limit, upper_limit),
        QueryMsg::PendingLiquidity {
            user,
            lower_limit,
            upper_limit,
        } => pending_liquidity(deps, user, lower_limit, upper_limit),
        QueryMsg::PoolReserves {} => pool_reserves(deps),
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
