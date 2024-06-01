use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::factory::{AllPoolsResponse, GetPoolResponse, PoolVlpResponse, StateResponse},
};

use crate::state::{STATE, VLP_TO_POOL};

// Returns the Pair Info of the Pair in the pool
pub fn get_pool(deps: Deps, vlp: String) -> Result<Binary, ContractError> {
    let pool = VLP_TO_POOL.load(deps.storage, vlp)?;
    Ok(to_json_binary(&GetPoolResponse { pool })?)
}
pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        chain_id: state.chain_id,
        router_contract: state.router_contract,
        admin: state.admin,
        pool_code_id: state.pool_code_id,
        hub_channel: state.hub_channel,
    })?)
}
pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let pools = VLP_TO_POOL
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let item = item.unwrap();
            PoolVlpResponse {
                pool: item.1.clone(),
                vlp: item.0,
            }
        })
        .collect();

    to_json_binary(&AllPoolsResponse { pools }).map_err(Into::into)
}
