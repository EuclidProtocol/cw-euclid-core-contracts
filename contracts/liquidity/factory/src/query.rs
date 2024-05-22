use cosmwasm_std::{to_json_binary, Binary, Deps, StdResult};
use euclid::msgs::factory::GetPoolResponse;

use crate::state::{CONNECTION_COUNTS, STATE, TIMEOUT_COUNTS, VLP_TO_POOL};

// Returns the Pair Info of the Pair in the pool
pub fn get_pool(deps: Deps, vlp: String) -> StdResult<Binary> {
    let pool = VLP_TO_POOL.load(deps.storage, vlp)?;
    to_json_binary(&GetPoolResponse { pool })
}
pub fn query_state(deps: Deps) -> StdResult<Binary> {
    let state = STATE.load(deps.storage)?;
    to_json_binary(&state)
}

pub fn query_connection_count(deps: Deps, channel_id: String) -> StdResult<Binary> {
    let count = CONNECTION_COUNTS
        .may_load(deps.storage, channel_id)?
        .unwrap_or(0);
    to_json_binary(&count)
}

pub fn query_timeout_count(deps: Deps, channel_id: String) -> StdResult<Binary> {
    let count = TIMEOUT_COUNTS
        .may_load(deps.storage, channel_id)?
        .unwrap_or(0);
    to_json_binary(&count)
}

pub fn query_pool_address(deps: Deps, vlp_address: String) -> StdResult<Binary> {
    let pool_address = VLP_TO_POOL.load(deps.storage, vlp_address)?;
    to_json_binary(&pool_address)
}
