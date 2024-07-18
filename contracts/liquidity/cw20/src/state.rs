use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Item;
use euclid::token::Pair;

#[cw_serde]
pub struct State {
    pub token_pair: Pair,
    pub factory_address: Addr,
    pub vlp: String,
}

pub const STATE: Item<State> = Item::new("state");
