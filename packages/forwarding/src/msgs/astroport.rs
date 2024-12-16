use astroport::router::SwapOperation;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use euclid::token::TokenType;

#[cw_serde]
pub struct InstantiateMsg {
    pub astro_router: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    Swap(SwapMsg),
    Receive(Cw20ReceiveMsg),
}

#[cw_serde]
pub enum Cw20HookMsg {
    Swap(SwapMsg),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct SwapMsg {
    pub operations: Option<Vec<SwapOperation>>,
    pub max_spread: Option<Decimal>,
    pub minimum_receive: Uint128,
    pub to_token: TokenType,
    pub forwarding_message: Option<Binary>,
    pub reciepient: Addr,
}
