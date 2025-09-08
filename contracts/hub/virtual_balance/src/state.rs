use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{
    msgs::virtual_balance::{Allowance, State},
    virtual_balance::SerializedBalanceKey,
};

pub const STATE: Item<State> = Item::new("state");

pub const BALANCES: Map<SerializedBalanceKey, Uint128> = Map::new("snapshot_balances");

// Allowance is stored as a map of balance key to allowance. It allows another user to spend on behalf of the owner.
// Only 1 allowance per balance key is allowed at a time.
pub const ALLOWANCES: Map<SerializedBalanceKey, Allowance> = Map::new("allowances");
