use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint256};
use cw_storage_plus::Item;
use forwarding::msgs::common_old::TokenType;
use forwarding::msgs::osmosis::SwapMsg;
#[cw_serde]
pub struct State {
    pub osmo_router_address: Addr,
}

pub const STATE: Item<State> = Item::new("state");

#[cw_serde]
pub struct ForwardingState {
    pub from_token: TokenType,
    pub from_amount: Uint256,
    pub previous_balance: Uint256,
    pub swap_msg: SwapMsg,
}

pub const FORWARDING_STATE: Item<ForwardingState> = Item::new("forwarding_state");
