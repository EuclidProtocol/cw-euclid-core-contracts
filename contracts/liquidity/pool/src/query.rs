use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::pool::{GetPairInfoResponse, GetPendingSwapsResponse, GetVLPResponse},
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
    lower_limit: u32,
    upper_limit: u32,
) -> Result<Binary, ContractError> {
    // Fetch pending swaps for user
    let pending_swaps = PENDING_SWAPS
        .may_load(deps.storage, user.clone())?
        .unwrap_or_default();
    // Get the upper limit
    let upper_limit = upper_limit as usize;
    // Get the lower limit
    let lower_limit = lower_limit as usize;
    // Get the pending swaps within the range
    let pending_swaps = pending_swaps[lower_limit..upper_limit].to_vec();
    // Return the response
    Ok(to_json_binary(&GetPendingSwapsResponse { pending_swaps })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_liquidity(
    deps: Deps,
    user: String,
    lower_limit: u32,
    upper_limit: u32,
) -> Result<Binary, ContractError> {
    let pending_liquidity = PENDING_LIQUIDITY
        .may_load(deps.storage, user.clone())?
        .unwrap_or_default();
    let upper_limit = upper_limit as usize;
    let lower_limit = lower_limit as usize;
    let pending_liquidity = pending_liquidity[lower_limit..upper_limit].to_vec();
    Ok(to_json_binary(&pending_liquidity)?)
}

// Returns the current reserves of tokens in the pool
pub fn pool_reserves(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let reserves = (state.reserve_1, state.reserve_2);
    Ok(to_json_binary(&reserves)?)
}
