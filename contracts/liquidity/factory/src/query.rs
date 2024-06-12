use cosmwasm_std::{to_json_binary, Binary, Deps, Order};
use euclid::{
    error::ContractError,
    msgs::{
        factory::{AllPoolsResponse, GetPoolResponse, PoolVlpResponse, StateResponse},
        pool::{GetPendingLiquidityResponse, GetPendingSwapsResponse},
    },
};

use crate::state::{PENDING_LIQUIDITY, PENDING_SWAPS, STATE, VLP_TO_POOL};

// Returns the Pair Info of the Pair in the pool
pub fn get_pool(deps: Deps, vlp: String) -> Result<Binary, ContractError> {
    let pair_info = VLP_TO_POOL.load(deps.storage, vlp)?;
    Ok(to_json_binary(&GetPoolResponse { pair_info })?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        chain_id: state.chain_id,
        router_contract: state.router_contract,
        admin: state.admin,
        // pool_code_id: state.pool_code_id,
        hub_channel: state.hub_channel,
    })?)
}
pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let pools = VLP_TO_POOL
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let item = item.unwrap();
            PoolVlpResponse {
                pair_info: item.1.clone(),
                vlp: item.0,
            }
        })
        .collect();

    to_json_binary(&AllPoolsResponse { pools }).map_err(Into::into)
}

// Returns the pending swaps for this pair with pagination
pub fn pending_swaps(
    deps: Deps,
    user: String,
    _lower_limit: Option<u128>,
    _upper_limit: Option<u128>,
) -> Result<Binary, ContractError> {
    // Fetch pending swaps for user
    let pending_swaps = PENDING_SWAPS
        .prefix(user)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|k| k.unwrap().1)
        .collect();

    Ok(to_json_binary(&GetPendingSwapsResponse { pending_swaps })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_liquidity(
    deps: Deps,
    user: String,
    _lower_limit: Option<u128>,
    _upper_limit: Option<u128>,
) -> Result<Binary, ContractError> {
    let pending_liquidity = PENDING_LIQUIDITY
        .prefix(user)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|k| k.unwrap().1)
        .collect();

    Ok(to_json_binary(&GetPendingLiquidityResponse {
        pending_liquidity,
    })?)
}
