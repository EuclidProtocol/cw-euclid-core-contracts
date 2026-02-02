use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{
    cross_chain_user::CrossChainUser, msgs::virtual_balance::msg::State,
    voucher::SerializedBalanceKey,
};

pub const STATE: Item<State> = Item::new("state");

pub const BALANCES: Map<SerializedBalanceKey, Uint128> = Map::new("balances");

#[cw_serde]
pub struct Allowance {
    pub spender: CrossChainUser,
    pub amount: Uint128,
}

// Allowance is stored as a map of balance key to allowance. It allows another user to spend on behalf of the owner.
// Only 1 allowance per balance key is allowed at a time.
pub const ALLOWANCES: Map<SerializedBalanceKey, Allowance> = Map::new("allowances");
