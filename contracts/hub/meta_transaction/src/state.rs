use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{admin::EuclidAdmin, msgs::meta_transaction::msg::State};

pub const STATE: Item<State> = Item::new("state");
pub const ADMIN: Item<EuclidAdmin> = Item::new("admin");

// (sender_key: "chainuid:address", nonce) -> block height when it was relayed
pub const NONCES: Map<(String, String), Uint128> = Map::new("nonces");
