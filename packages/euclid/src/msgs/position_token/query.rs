use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::utils::pagination::Pagination;

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(OwnerOfResponse)]
    OwnerOf { token_id: String },
    #[returns(TokenInfoResponse)]
    TokenInfo { token_id: String },
    #[returns(PositionInfoResponse)]
    PositionInfo { token_id: String },
    #[returns(TokensResponse)]
    TokensByOwner {
        owner: String,
        pagination: Pagination<String>,
    },
    #[returns(TokensResponse)]
    AllTokens { pagination: Pagination<String> },
    #[returns(StateResponse)]
    State {},
}

#[cw_serde]
pub struct OwnerOfResponse {
    pub owner: String,
}

#[cw_serde]
pub struct TokenInfoResponse {
    pub owner: String,
    pub token_uri: Option<String>,
}

#[cw_serde]
pub struct PositionInfoResponse {
    pub liquidity: Uint128,
}

#[cw_serde]
pub struct TokensResponse {
    pub tokens: Vec<String>,
}

#[cw_serde]
pub struct StateResponse {
    pub name: String,
    pub symbol: String,
    pub factory: Addr,
    pub vlp_address: String,
    pub total_tokens: u64,
}
