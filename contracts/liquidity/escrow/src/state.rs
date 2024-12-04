use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use euclid::token::{Token, TokenType};
use secret_toolkit::{
    serialization::Json,
    storage::{Item, Keymap},
};
#[cw_serde]
pub struct State {
    pub token_id: Token,
    pub factory_address: Addr,
    pub total_amount: Uint128,
}

pub const STATE: Item<State> = Item::new(b"state");
pub const ALLOWED_DENOMS: Item<Vec<TokenType>,Json> = Item::new(b"allowed_denoms");

pub const DENOM_TO_AMOUNT: Keymap<String, Uint128, Json> = Keymap::new(b"denom_to_amount");
