use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::chain::ChainUid;
use relayer::{msgs::State, Validator};

pub const STATE: Item<State> = Item::new("state");

pub const VALIDATORS: Map<ChainUid, Vec<Validator>> = Map::new("validators");
// nonce -> block height when it was relayed
pub const NONCES: Map<String, Uint128> = Map::new("nonces");
