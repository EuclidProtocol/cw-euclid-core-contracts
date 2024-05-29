#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, Binary, Deps, DepsMut, Env, MessageInfo, Response, Uint128};
use cw2::set_contract_version;

use crate::state::{State, POOLS, STATE};
use euclid::error::ContractError;
use euclid::msgs::vlp::{ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::query::{query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap};
// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        pair: msg.pair,
        router: info.sender.to_string(),
        fee: msg.fee,
        last_updated: 0,
        total_reserve_1: msg.pool.reserve_1,
        total_reserve_2: msg.pool.reserve_2,
        total_lp_tokens: Uint128::zero(),
        lq_ratio: msg.lq_ratio,
    };
    ensure!(
        !state.lq_ratio.is_zero(),
        ContractError::InvalidLiquidityRatio {}
    );

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;
    // stores initial pool to map
    POOLS.save(deps.storage, &msg.pool.chain, &msg.pool)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        // ExecuteMsg::RegisterPool {pool} => execute::register_pool(deps, info, pool),
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
