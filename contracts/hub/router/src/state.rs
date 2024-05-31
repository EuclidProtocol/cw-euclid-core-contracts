use cosmwasm_schema::cw_serde;
use cw_storage_plus::{Item, Map};
use euclid::token::Token;

#[cw_serde]
pub struct State {
    // Contract admin
    pub admin: String,
    // Pool Code ID
    pub vlp_code_id: u64,
}

pub const STATE: Item<State> = Item::new("state");

// Convert it to multi index map?
pub const VLPS: Map<(Token, Token), String> = Map::new("vlps");

// chain id to factory map
pub const FACTORIES: Map<String, String> = Map::new("factories");

// chain id to channel
pub const CHANNELS: Map<String, String> = Map::new("channels");

/// (channel_id) -> count. Reset on channel closure.
pub const CONNECTION_COUNTS: Map<String, u32> = Map::new("connection_counts");
/// (channel_id) -> timeout_count. Reset on channel closure.
pub const TIMEOUT_COUNTS: Map<String, u32> = Map::new("timeout_count");
