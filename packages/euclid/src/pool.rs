use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::token::PairInfo;

pub const MINIMUM_LIQUIDITY: u128 = 1000;

#[cw_serde]
pub struct Pool {
    // The chain where the pool is deployed
    pub chain: String,
    // The PairInfo of the pool
    pub pair: PairInfo,
    // The total reserve of token_1
    pub reserve_1: Uint128,
    // The total reserve of token_2
    pub reserve_2: Uint128,
}

// Request to create pool saved in state to manage during acknowledgement
#[cw_serde]
pub struct PoolRequest {
    // The chain where the pool is deployed
    pub chain: String,
    // Pool request id
    pub pool_rq_id: String,
    // The channel where the pool is deployed
    pub channel: String,
}

// Function to extract sender from pool_rq_id
pub fn extract_sender(pool_rq_id: &str) -> String {
    let parts: Vec<&str> = pool_rq_id.split('-').collect();
    parts[0].to_string()
}

// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct LiquidityResponse {
    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub mint_lp_tokens: Uint128,
}

// Struct to handle Acknowledgement Response for a Pool Creation Request
#[cw_serde]
pub struct PoolCreationResponse {
    pub vlp_contract: String,
}
