use super::common_old::{EuclidReceive, TokenType};
use astroport::router::SwapOperation;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {
    pub astro_router: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    EuclidReceive(EuclidReceive),
    Swap(SwapMsg),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct SwapMsg {
    pub operations: Vec<SwapOperation>,
    pub max_spread: Option<Decimal>,
    pub forwarding_msg: Option<Binary>,
    pub to_token: TokenType,
    pub minimum_receive: Uint128,
    pub recipient: String,
}

#[cw_serde]
pub struct MigrateMsg {}
