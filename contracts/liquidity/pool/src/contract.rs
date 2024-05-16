#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
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
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        vlp_contract: msg.vlp_contract.clone(),
        pair: msg.token_pair.clone(),
        pair_info: msg.pair_info.clone(),
        reserve_1: msg.pool.reserve_1,
        reserve_2: msg.pool.reserve_2,
        // Store factory contract
        factory_contract: info.sender.clone().to_string(),
        chain_id: msg.chain_id,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut msgs = Vec::new();
    // Check if tokens are smart contract tokens to create transfer message
    if msg.pair_info.token_1.is_smart() {
        let msg = msg
            .pair_info
            .token_1
            .create_transfer_msg(msg.pool.reserve_1, env.contract.address.clone().to_string());
        msgs.push(msg);
    }

    if msg.pair_info.token_2.is_smart() {
        let msg = msg
            .pair_info
            .token_2
            .create_transfer_msg(msg.pool.reserve_1, env.contract.address.clone().to_string());
        msgs.push(msg);
    }

    // Validate for deposit of native tokens
    if msg.pair_info.token_1.is_native() {
        // Query the balance of the contract for the native token
        let balance = deps
            .querier
            .query_balance(
                env.contract.address.clone(),
                msg.pair_info.token_1.get_denom(),
            )
            .unwrap();
        // Verify that the balance is greater than the reserve added
        if balance.amount < msg.pool.reserve_1 {
            return Err(ContractError::InsufficientDeposit {});
        }
    }

    // Same for token 2
    if msg.pair_info.token_2.is_native() {
        let balance = deps
            .querier
            .query_balance(
                env.contract.address.clone(),
                msg.pair_info.token_2.get_denom(),
            )
            .unwrap();
        if balance.amount < msg.pool.reserve_2 {
            return Err(ContractError::InsufficientDeposit {});
        }
    }

    // Save the state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("token_1", msg.token_pair.token_1.id)
        .add_attribute("token_2", msg.token_pair.token_2.id)
        .add_attribute("factory_contract", info.sender.clone().to_string())
        .add_attribute("vlp_contract", msg.vlp_contract)
        .add_attribute("chain_id", "chain_id")
        .add_messages(msgs))
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
        } => execute::execute_swap_request(
            deps,
            info,
            env,
            asset,
            asset_amount,
            min_amount_out,
            channel,
            None,
        ),
        ExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            channel,
        } => execute::add_liquidity_request(
            deps,
            info,
            env,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            channel,
            None,
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
    if info.sender != state.factory_contract {
        return Err(ContractError::Unauthorized {});
    }

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
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::PairInfo {} => query::pair_info(deps),
        QueryMsg::PendingSwapsUser {
            user,
            upper_limit,
            lower_limit,
        } => query::pending_swaps(deps, user, lower_limit, upper_limit),
    }
}
