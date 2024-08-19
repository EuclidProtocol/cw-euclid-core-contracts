use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{msgs::virtual_balance::State, virtual_balance::SerializedBalanceKey};

pub const STATE: Item<State> = Item::new("state");

pub const BALANCES: Map<SerializedBalanceKey, Uint128> = Map::new("snapshot_balances");
