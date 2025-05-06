use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use relayer::msgs::State;

pub const STATE: Item<State> = Item::new("state");

// nonce -> block height when it was relayed
pub const NONCES: Map<String, Uint128> = Map::new("nonces");
