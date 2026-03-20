use cosmwasm_std::Uint256;
use cw_storage_plus::{Item, Map};
use euclid::admin::EuclidAdmin;
use euclid::chain::ChainUid;
use relayer::{msgs::State, Validator};

pub const STATE: Item<State> = Item::new("state");
pub const ADMIN: Item<EuclidAdmin> = Item::new("admin");

pub const VALIDATORS: Map<ChainUid, Vec<Validator>> = Map::new("validators");
// nonce -> block height when it was relayed
pub const NONCES: Map<String, Uint256> = Map::new("nonces");
