use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::token::{Pair, PairWithDenom};

#[cw_serde]
pub struct AddLiquidityRequest {
    pub sender: String,
    pub tx_id: String,

    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub pair_info: PairWithDenom,
}

// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct AddLiquidityResponse {
    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub mint_lp_tokens: Uint128,
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
    pub vlp_address: String,
}

#[cw_serde]
pub struct RemoveLiquidityRequest {
    pub sender: String,
    pub tx_id: String,

    pub lp_allocation: Uint128,
    pub pair: Pair,
}
// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct RemoveLiquidityResponse {
    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub burn_lp_tokens: Uint128,
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
    pub vlp_address: String,
}
