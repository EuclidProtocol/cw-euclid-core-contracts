use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::token::PairWithDenom;

#[cw_serde]
pub struct LiquidityTxInfo {
    pub sender: String,
    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub pair_info: PairWithDenom,
    pub liquidity_id: String,
}
