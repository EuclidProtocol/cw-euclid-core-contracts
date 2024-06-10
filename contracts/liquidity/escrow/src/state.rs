use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};
use euclid::{
    msgs::escrow::AmountAndType,
    token::{Token, TokenInfo},
};
#[cw_serde]
pub struct State {
    pub token_id: Token,
    pub factory_address: Addr,
}

pub const STATE: Item<State> = Item::new("state");
pub const ALLOWED_DENOMS: Item<Vec<String>> = Item::new("allowed_denoms");

pub const DENOM_TO_AMOUNT: Map<String, AmountAndType> = Map::new("denom_to_amount");
