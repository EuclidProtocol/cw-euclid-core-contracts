use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use euclid::msgs::position_token::{PositionInfo, State, TokenInfo};

pub const STATE: Item<State> = Item::new("state");
/// Map for token info
pub const TOKENS: Map<&str, TokenInfo> = Map::new("tokens");

/// Map for position info
pub const POSITION_INFO: Map<&str, PositionInfo> = Map::new("position_info");

/// Per-owner token index. Each (owner, token_id) pair is a separate storage key,
/// avoiding unbounded Vec deserialization on every operation.
pub const OWNER_TOKEN_SET: Map<(&Addr, &str), cosmwasm_std::Empty> = Map::new("owner_token_set");
