#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, Uint128};
use cw2::set_contract_version;

use crate::execute;
use crate::state::{State, STATE};
use euclid::error::ContractError;
use euclid::msgs::vlp::{ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::query::{query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap};
// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        pair: msg.pair,
        router: info.sender.to_string(),
        fee: msg.fee,
        last_updated: 0,
        total_reserve_1: Uint128::zero(),
        total_reserve_2: Uint128::zero(),
        total_lp_tokens: Uint128::zero(),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    let response = msg.execute.map_or(Ok(Response::default()), |execute_msg| {
        execute(deps, env.clone(), info.clone(), execute_msg)
    })?;
    Ok(response
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool {
            chain_id,
            pair_info,
        } => execute::register_pool(deps, env, chain_id, pair_info),
        ExecuteMsg::Swap {
            chain_id,
            asset,
            asset_amount,
            min_token_out,
            swap_id,
        } => execute::execute_swap(deps, chain_id, asset, asset_amount, min_token_out, swap_id),
        ExecuteMsg::AddLiquidity {
            chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
        } => execute::add_liquidity(
            deps,
            chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
        ),
        ExecuteMsg::RemoveLiquidity {
            chain_id,
            lp_allocation,
        } => execute::remove_liquidity(deps, chain_id, lp_allocation),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::SimulateSwap {
            asset,
            asset_amount,
        } => query_simulate_swap(deps, asset, asset_amount),
        QueryMsg::Liquidity {} => query_liquidity(deps),
        QueryMsg::Fee {} => query_fee(deps),
        QueryMsg::Pool { chain_id } => query_pool(deps, chain_id),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
    }
}
