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
pub const OWNER_TOKENS: Map<&Addr, Vec<String>> = Map::new("owner_tokens");
pub const ALL_TOKENS: Item<Vec<String>> = Item::new("all_tokens");
