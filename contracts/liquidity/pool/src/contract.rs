#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;

use crate::state::{State, STATE};
use crate::{execute, query};
use euclid::error::ContractError;
use euclid::msgs::pool::{CallbackExecuteMsg, ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:pool";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        vlp_contract: msg.vlp_contract.clone(),
        pair_info: msg.pool.pair.clone(),
        reserve_1: msg.pool.reserve_1,
        reserve_2: msg.pool.reserve_2,
        // Store factory contract
        factory_contract: info.sender.clone().to_string(),
        chain_id: msg.chain_id,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Save the state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("token_1", msg.pool.pair.token_1.get_token().id)
        .add_attribute("token_2", msg.pool.pair.token_2.get_token().id)
        .add_attribute("factory_contract", info.sender.clone().to_string())
        .add_attribute("vlp_contract", msg.vlp_contract)
        .add_attribute("chain_id", "chain_id"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ExecuteSwap {
            asset,
            asset_amount,
            min_amount_out,
            channel,
            timeout,
        } => execute::execute_swap_request(
            deps,
            info,
            env,
            asset,
            asset_amount,
            min_amount_out,
            channel,
            None,
            timeout,
        ),
        ExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            channel,
            timeout,
        } => execute::add_liquidity_request(
            deps,
            info,
            env,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            channel,
            None,
            timeout,
        ),
        ExecuteMsg::Receive(msg) => execute::receive_cw20(deps, env, info, msg),
        ExecuteMsg::Callback(callback_msg) => {
            handle_callback_execute(deps, env, info, callback_msg)
        }
    }
}

fn handle_callback_execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: CallbackExecuteMsg,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Only factory contract can call this contract
    ensure!(
        info.sender == state.factory_contract,
        ContractError::Unauthorized {}
    );

    match msg {
        CallbackExecuteMsg::CompleteSwap { swap_response } => {
            execute::execute_complete_swap(deps, swap_response)
        }
        CallbackExecuteMsg::RejectSwap { swap_id, error } => {
            execute::execute_reject_swap(deps, swap_id, error)
        }
        CallbackExecuteMsg::CompleteAddLiquidity {
            liquidity_response,
            liquidity_id,
        } => execute::execute_complete_add_liquidity(deps, liquidity_response, liquidity_id),
        CallbackExecuteMsg::RejectAddLiquidity {
            liquidity_id,
            error,
        } => execute::execute_reject_add_liquidity(deps, liquidity_id, error),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::PairInfo {} => query::pair_info(deps),
        QueryMsg::GetVlp {} => query::get_vlp(deps),
        QueryMsg::PendingSwapsUser {
            user,
            upper_limit,
            lower_limit,
        } => query::pending_swaps(deps, user, lower_limit, upper_limit),
        QueryMsg::PendingLiquidity {
            user,
            lower_limit,
            upper_limit,
        } => query::pending_liquidity(deps, user, lower_limit, upper_limit),
        QueryMsg::PoolReserves {} => query::pool_reserves(deps),
    }
}
