use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use euclid::token::{Token, TokenType};
use secret_storage_plus::{Item, Map};
#[cw_serde]
pub struct State {
    pub token_id: Token,
    pub factory_address: Addr,
    pub total_amount: Uint128,
}

pub const STATE: Item<State> = Item::new("state");
pub const ALLOWED_DENOMS: Item<Vec<TokenType>> = Item::new("allowed_denoms");

pub const DENOM_TO_AMOUNT: Map<String, Uint128> = Map::new("denom_to_amount");
