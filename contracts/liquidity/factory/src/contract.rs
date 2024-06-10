#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::error::ContractError;

use crate::execute::{
    add_liquidity_request, execute_request_pool_creation, execute_swap_request, receive_cw20,
};
use crate::query::{
    get_pool, get_vlp, pair_info, pending_liquidity, pending_swaps, pool_reserves, query_all_pools,
    query_state,
};
use crate::reply;
use crate::reply::INSTANTIATE_REPLY_ID;
use crate::state::{State, STATE};
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
        // pool_code_id: msg.pool_code_id,
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
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        // ExecuteMsg::AddLiquidity {
        //     token_1_liquidity,
        //     token_2_liquidity,
        //     slippage_tolerance,
        //     liquidity_id,
        //     timeout,
        //     vlp_address,
        // } => execute_add_liquidity(
        //     deps,
        //     env,
        //     info,
        //     token_1_liquidity,
        //     token_2_liquidity,
        //     slippage_tolerance,
        //     liquidity_id,
        //     timeout,
        //     vlp_address,
        // ),
        // ExecuteMsg::ExecuteSwap {
        //     asset,
        //     asset_amount,
        //     min_amount_out,
        //     swap_id,
        //     timeout,
        //     vlp_address,
        // } => execute_swap(
        //     deps,
        //     env,
        //     info,
        //     asset,
        //     asset_amount,
        //     min_amount_out,
        //     swap_id,
        //     timeout,
        //     vlp_address,
        // ),
        ExecuteMsg::RequestPoolCreation { pair_info, timeout } => {
            execute_request_pool_creation(deps, env, info, pair_info, timeout)
        }
        // ExecuteMsg::UpdatePoolCodeId { new_pool_code_id } => {
        //     execute_update_pool_code_id(deps, info, new_pool_code_id)
        // }
        // Pool Execute Msgs //
        ExecuteMsg::AddLiquidityRequest {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            timeout,
        } => add_liquidity_request(
            deps,
            info,
            env,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            None,
            timeout,
        ),
        ExecuteMsg::ExecuteSwapRequest {
            asset,
            asset_amount,
            min_amount_out,
            timeout,
        } => execute_swap_request(
            &mut deps,
            info,
            env,
            asset,
            asset_amount,
            min_amount_out,
            None,
            timeout,
        ),
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
