use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use euclid::{pool::Pool, token::{Pair, PairInfo, Token, TokenInfo}};

#[cw_serde]
pub struct InstantiateMsg {
    pub vlp_contract: String,
    pub token_pair: Pair,
    pub pair_info: PairInfo,
    pub pool: Pool,
    pub chain_id: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    ExecuteSwap {
        asset: TokenInfo, 
        asset_amount: Uint128,
        min_amount_out: Uint128,
        channel: String,

    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetCountResponse {
    pub count: i32,
}
