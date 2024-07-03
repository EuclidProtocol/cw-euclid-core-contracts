use cosmwasm_schema::cw_serde;
use cosmwasm_std::{IbcTimeout, Uint128};

use crate::token::{Token, TokenWithDenom};

// Struct that stores a certain swap info
#[cw_serde]
pub struct SwapRequest {
    pub sender: String,
    pub tx_id: String,

    // The asset being swapped
    pub asset_in: TokenWithDenom,
    // The asset being received
    pub asset_out: Token,
    // The amount of asset being swapped
    pub amount_in: Uint128,
    // The min amount of asset being received
    pub min_amount_out: Uint128,
    // All the swaps needed for assent_in <> asset_out
    pub swaps: Vec<NextSwap>,
    // The timeout specified for the swap
    pub timeout: IbcTimeout,

    pub to_address: String,
    pub to_chain_uid: String,
}

#[cw_serde]
pub struct NextSwap {
    pub vlp_address: String,
    pub test_fail: Option<bool>,
}

#[cw_serde]
pub struct SwapResponse {
    pub amount_out: Uint128,
}
