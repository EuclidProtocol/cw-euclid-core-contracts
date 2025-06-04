use cosmwasm_schema::cw_serde;
use cw_storage_plus::Item;

#[cw_serde]
pub struct State {
    pub router: String,
    pub virtual_balance: String,
    pub vlp: String,
    pub admin: String,
}

pub const STATE: Item<State> = Item::new("state");
