use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};

use crate::{
    cross_chain_user::CrossChainUser,
    msgs::vlp::base::PoolKey,
    token::{Pair, PairWithAmount, PairWithDenomAndAmount},
};

#[cw_serde]
pub struct AddLiquidityRequest {
    pub sender: String,
    pub tx_id: String,
    pub pair_info: PairWithDenomAndAmount,
}

// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct AddLiquidityResponse {
    pub mint_lp_tokens: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct RemoveLiquidityRequest {
    pub sender: String,
    pub tx_id: String,

    pub lp_allocation: Uint128,
    pub pair: Pair,
    pub lp_token: Addr,
}
// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct RemoveLiquidityResponse {
    pub liquidity_removed: PairWithAmount,
    pub burn_lp_tokens: Uint128,
    pub vlp_address: String,
}

#[cw_serde]
pub struct ConcentratedAddLiquidityResponse {
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub liquidity_delta: Uint128,
    pub mint_lp_tokens: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct ConcentratedRemoveLiquidityResponse {
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub liquidity_removed: PairWithAmount,
    pub liquidity_delta: Uint128,
    pub liquidity_after: Uint128,
    pub burn_lp_tokens: Uint128,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
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
