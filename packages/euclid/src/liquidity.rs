use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128, Uint256};

use crate::{
    cross_chain_user::CrossChainUser,
    msgs::vlp::base::PoolKey,
    token::{Pair, PairWithAmount, PairWithDenomAndAmount},
};

/// Tick bounds derived from the Q64.96 fixed-point representation of sqrt_price.
/// price = 1.0001^tick, stored as sqrt_price_x96 = sqrt(1.0001^tick) * 2^96.
/// MIN_TICK is the lowest tick whose sqrt_price_x96 remains non-zero (~2.94e-39 price).
/// MAX_TICK is the highest tick that fits in Uint256 without overflow (~3.40e38 price).
pub const MIN_TICK: i64 = -887_272;
pub const MAX_TICK: i64 = 887_272;

#[cw_serde]
pub struct AddLiquidityRequest {
    pub sender: String,
    pub tx_id: String,
    pub pair_info: PairWithDenomAndAmount,
}

// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct AddLiquidityResponse {
    pub mint_lp_tokens: Uint256,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct RemoveLiquidityRequest {
    pub sender: String,
    pub tx_id: String,

    pub lp_allocation: Uint256,
    pub pair: Pair,
    pub lp_token: Addr,
}
// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct RemoveLiquidityResponse {
    pub liquidity_removed: PairWithAmount,
    pub burn_lp_tokens: Uint256,
    pub vlp_address: String,
}

#[cw_serde]
pub struct ConcentratedAddLiquidityResponse {
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub position_id: Uint128,
    pub liquidity_delta: Uint128,
}

#[cw_serde]
pub struct ConcentratedRemoveLiquidityResponse {
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub liquidity_removed: PairWithAmount,
    pub liquidity_delta: Uint128,
    pub liquidity_after: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    /// True when the VLP deleted the position from storage (zero liquidity and zero owed fees).
    pub position_burned: bool,
}

#[cw_serde]
pub struct ConcentratedCollectFeesResponse {
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub amount_0: Uint128,
    pub amount_1: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub recipient: CrossChainUser,
}

#[cw_serde]
pub struct ConcentratedCollectProtocolFeesResponse {
    pub pool_key: PoolKey,
    pub amount_0: Uint128,
    pub amount_1: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub recipient: CrossChainUser,
}
