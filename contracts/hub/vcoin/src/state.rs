use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{msgs::vcoin::State, vcoin::SerializedBalanceKey};

pub const STATE: Item<State> = Item::new("state");

pub const BALANCES: Map<SerializedBalanceKey, Uint128> = Map::new("snapshot_balances");
