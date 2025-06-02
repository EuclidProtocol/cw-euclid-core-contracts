use cosmwasm_schema::cw_serde;
use cw_storage_plus::Item;

#[cw_serde]
pub struct State {
    // Router Contract
    pub router: String,
    // Virtual Coin Contract
    pub virtual_balance: String,
    pub admin: String,
}

pub const STATE: Item<State> = Item::new("state");
