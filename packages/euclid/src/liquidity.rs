use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint256};

use crate::{
    cross_chain_user::CrossChainUser,
    token::{Pair, PairWithAmount, PairWithDenomAndAmount, TokenWithDenom},
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
    pub mint_lp_tokens: Uint256,
    pub vlp_address: String,
    pub tx_id: String,
    pub sender: CrossChainUser,
}

#[cw_serde]
pub struct SingleSidedLiquidityRequest {
    pub sender: String,
    pub tx_id: String,
    pub asset_in: TokenWithDenom,
    // Post-partner-fee deposit amount: the actual amount that crosses IBC
    // and ends up in escrow on success.
    pub amount_in: Uint256,
    // Partner fee retained at the factory until the ack resolves.
    pub partner_fee_amount: Uint256,
    pub partner_fee_recipient: Addr,
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
