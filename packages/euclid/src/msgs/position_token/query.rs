use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::utils::pagination::Pagination;

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
#[cfg_attr(feature = "cross-vm", derive(cross_vm_macros::CwQueryFns))]
#[cfg_attr(feature = "cross-vm", cross_vm(trait_name = "PositionTokenQueryFns"))]
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
    #[returns(crate::build_info::BuildInfoResponse)]
    GetBuildInfo {},
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
    pub vlp_address: String,
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
    pub total_tokens: u64,
}
