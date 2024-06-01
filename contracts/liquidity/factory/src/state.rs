use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut};
use cw_storage_plus::{Item, Map};
use euclid::{
    error::ContractError,
    pool::{generate_id, PoolRequest},
};

#[cw_serde]
pub struct State {
    // The Unique Chain Identifier
    // THIS IS DIFFERENT THAN THE CHAIN_ID OF THE CHAIN, THIS REPRESENTS A UNIQUE IDENTIFIER FOR THE CHAIN
    // IN THE EUCLID ECOSYSTEM
    pub chain_id: String,
    // The Router Contract Address on the Virtual Settlement Layer
    pub router_contract: String,
    // Contract admin
    pub admin: String,
    // Pool Code ID
    pub pool_code_id: u64,
}

pub const STATE: Item<State> = Item::new("state");

/// (channel_id) -> count. Reset on channel closure.
pub const CONNECTION_COUNTS: Map<String, u32> = Map::new("connection_counts");
/// (channel_id) -> timeout_count. Reset on channel closure.
pub const TIMEOUT_COUNTS: Map<String, u32> = Map::new("timeout_count");

// Map VLP address to Pool address
pub const VLP_TO_POOL: Map<String, String> = Map::new("vlp_to_pool");

// Map sender of Pool request to Pool address
pub const POOL_REQUESTS: Map<String, PoolRequest> = Map::new("request_to_pool");

// Pool Requests Counter
pub const POOL_REQUEST_COUNT: Map<String, u128> = Map::new("request_to_pool_count");

pub fn generate_pool_req(
    deps: DepsMut,
    sender: &Addr,
    chain: String,
    channel: String,
) -> Result<PoolRequest, ContractError> {
    let count = POOL_REQUEST_COUNT
        .may_load(deps.storage, sender.to_string())?
        .unwrap_or_default();

    let pool_rq_id = generate_id(sender.as_str(), count);
    let pool_request = PoolRequest {
        chain,
        channel,
        pool_rq_id: pool_rq_id.clone(),
    };
    // If a pool request already exist, throw error, else create a new request
    POOL_REQUESTS.update(deps.storage, pool_rq_id, |existing| match existing {
        Some(_req) => Err(ContractError::PoolRequestAlreadyExists {}),
        None => Ok(pool_request.clone()),
    })?;
    POOL_REQUEST_COUNT.save(deps.storage, sender.to_string(), &count.wrapping_add(1))?;
    Ok(pool_request)
}
