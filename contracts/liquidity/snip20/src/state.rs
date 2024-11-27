use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use euclid::token::Pair;
use secret_storage_plus::Item;

#[cw_serde]
pub struct State {
    pub token_pair: Pair,
    pub factory_address: Addr,
    pub vlp: String,
}

pub const STATE: Item<State> = Item::new("state");
