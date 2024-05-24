use cosmwasm_std::{to_json_binary, Binary, Deps, Order};
use euclid::{
    error::ContractError,
    msgs::pool::{
        GetPairInfoResponse, GetPendingLiquidityResponse, GetPendingSwapsResponse,
        GetPoolReservesResponse, GetVLPResponse,
    },
};

use crate::state::{PENDING_LIQUIDITY, PENDING_SWAPS, STATE};

// Returns the Pair Info of the Pair in the pool
pub fn pair_info(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetPairInfoResponse {
        pair_info: state.pair_info,
    })?)
}

// Returns the Pair Info of the Pair in the pool
pub fn get_vlp(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetVLPResponse {
        vlp: state.vlp_contract,
    })?)
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

// Returns the current reserves of tokens in the pool
pub fn pool_reserves(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetPoolReservesResponse {
        reserve_1: state.reserve_1,
        reserve_2: state.reserve_2,
    })?)
}
