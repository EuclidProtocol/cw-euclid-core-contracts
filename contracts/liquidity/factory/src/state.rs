use cosmwasm_schema::cw_serde;
use cw_storage_plus::{Item, Map};
use euclid::pool::PoolRequest;

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