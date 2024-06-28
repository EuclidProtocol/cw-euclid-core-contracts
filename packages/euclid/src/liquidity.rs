use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::token::{Pair, PairWithDenom};

#[cw_serde]
pub struct LiquidityTxInfo {
    pub sender: String,
    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub pair_info: PairWithDenom,
    pub tx_id: String,
}
#[cw_serde]
pub struct RemoveLiquidityTxInfo {
    pub sender: String,
    pub lp_allocation: Uint128,
    pub pair: Pair,
    pub tx_id: String,
}
