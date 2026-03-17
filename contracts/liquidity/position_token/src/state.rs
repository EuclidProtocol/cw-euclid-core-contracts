use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct State {
    pub name: String,
    pub symbol: String,
    pub minter: Addr,
    pub admin: Addr,
    pub total_tokens: u64,
}

#[cw_serde]
pub struct TokenInfo {
    pub owner: Addr,
    pub token_uri: Option<String>,
}

pub const STATE: Item<State> = Item::new("state");
pub const TOKENS: Map<&str, TokenInfo> = Map::new("tokens");

/// Per-owner token index. Each (owner, token_id) pair is a separate storage key,
/// avoiding unbounded Vec deserialization on every operation.
pub const OWNER_TOKEN_SET: Map<(&Addr, &str), cosmwasm_std::Empty> = Map::new("owner_token_set");

/// Global token index. Each token_id is a separate storage key.
pub const ALL_TOKEN_SET: Map<&str, cosmwasm_std::Empty> = Map::new("all_token_set");
