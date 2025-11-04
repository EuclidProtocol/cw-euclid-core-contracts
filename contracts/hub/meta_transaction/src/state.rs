use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::msgs::meta_transaction::State;

pub const STATE: Item<State> = Item::new("state");

pub const AUTHORIZED_ADDRESSES: Item<Vec<Addr>> = Item::new("authorized_addresses");

// nonce -> block height when it was relayed
pub const NONCES: Map<String, Uint128> = Map::new("nonces");
