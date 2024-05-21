use cosmwasm_std::{to_json_binary, Binary, Deps, StdResult};
use euclid::msgs::factory::GetPoolResponse;

use crate::state::VLP_TO_POOL;

// Returns the Pair Info of the Pair in the pool
pub fn get_pool(deps: Deps, vlp: String) -> StdResult<Binary> {
    let pool = VLP_TO_POOL.load(deps.storage, vlp)?;
    to_json_binary(&GetPoolResponse { pool })
}
