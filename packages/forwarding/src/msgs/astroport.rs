use astroport::router::SwapOperation;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use euclid::{msgs::hook::EuclidReceive, token::TokenType};

#[cw_serde]
pub struct InstantiateMsg {
    pub astro_router: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    EuclidReceive(EuclidReceive),
}

#[cw_serde]
pub enum Cw20HookMsg {
    EuclidReceive(EuclidReceive),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct SwapMsg {
    pub operations: Vec<SwapOperation>,
    pub max_spread: Option<Decimal>,
    pub forwarding_msg: Option<EuclidReceive>,
    pub to_token: TokenType,
    pub minimum_receive: Uint128,
    pub recipient: String,
}
