#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::error::ContractError;

use crate::execute::{
    add_liquidity_request, execute_request_deregister_denom, execute_request_pool_creation,
    execute_request_register_denom, execute_swap_request, receive_cw20,
};
use crate::query::{get_pool, pending_liquidity, pending_swaps, query_all_pools, query_state};
use crate::reply;
use crate::reply::ESCROW_INSTANTIATE_REPLY_ID;
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
        hub_channel: None,
        escrow_code_id: msg.escrow_code_id,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", msg.router_contract)
        .add_attribute("chain_id", state.chain_id)
        .add_attribute("escrow_code_id", state.escrow_code_id.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RequestRegisterDenom { denom, token_id } => {
            execute_request_register_denom(deps, env, info, token_id, denom)
        }
        ExecuteMsg::RequestDeregisterDenom { denom, token_id } => {
            execute_request_deregister_denom(deps, env, info, token_id, denom)
        }
        ExecuteMsg::RequestPoolCreation { pair_info, timeout } => {
            execute_request_pool_creation(deps, env, info, pair_info, timeout)
        }
        ExecuteMsg::AddLiquidityRequest {
            vlp_address,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            timeout,
        } => add_liquidity_request(
            deps,
            info,
            env,
            vlp_address,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            None,
            timeout,
        ),
        ExecuteMsg::ExecuteSwapRequest {
            asset_in,
            asset_out,
            amount_in,
            min_amount_out,
            timeout,
            swaps,
        } => execute_swap_request(
            &mut deps,
            info,
            env,
            asset_in,
            asset_out,
            amount_in,
            min_amount_out,
            swaps,
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
        QueryMsg::GetEscrow { token_id } => get_pool(deps, token_id),
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
        // Pool Queries //
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
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        ESCROW_INSTANTIATE_REPLY_ID => reply::on_escrow_instantiate_reply(deps, msg),
        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}

#[cfg(test)]
mod tests {}
