use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;
use euclid::token::TokenType;
use forwarding::msgs::duality::SwapMsg;
#[cw_serde]
pub struct State {
    pub duality_router_address: Addr,
}

pub const STATE: Item<State> = Item::new("state");

#[cw_serde]
pub struct ForwardingState {
    pub from_token: TokenType,
    pub from_amount: Uint128,
    pub previous_balance: Uint128,
    pub swap_msg: SwapMsg,
}

pub const FORWARDING_STATE: Item<ForwardingState> = Item::new("forwarding_state");
