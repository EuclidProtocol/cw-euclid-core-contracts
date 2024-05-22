use cosmwasm_std::{to_json_binary, Binary, Deps, StdResult};
use euclid::msgs::pool::{GetPairInfoResponse, GetPendingSwapsResponse, GetVLPResponse};

use crate::state::{PENDING_SWAPS, STATE};

// Returns the Pair Info of the Pair in the pool
pub fn pair_info(deps: Deps) -> StdResult<Binary> {
    let state = STATE.load(deps.storage)?;
    to_json_binary(&GetPairInfoResponse {
        pair_info: state.pair_info,
    })
}

// Returns the Pair Info of the Pair in the pool
pub fn get_vlp(deps: Deps) -> StdResult<Binary> {
    let state = STATE.load(deps.storage)?;
    to_json_binary(&GetVLPResponse {
        vlp: state.vlp_contract,
    })
}

// Returns the pending swaps for this pair with pagination
pub fn pending_swaps(
    deps: Deps,
    user: String,
    lower_limit: u32,
    upper_limit: u32,
) -> StdResult<Binary> {
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
    to_json_binary(&GetPendingSwapsResponse { pending_swaps })
}
