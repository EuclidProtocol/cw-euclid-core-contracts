use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};

use crate::{
    recipient::Recipient,
    token::{Token, TokenWithDenom},
};

// Struct that stores a certain swap info
#[cw_serde]
pub struct SwapRequest {
    pub sender: String,
    pub tx_id: String,

    // The asset being swapped
    pub asset_in: TokenWithDenom,
    // The amount of asset_in being swapped
    pub amount_in: Uint128,
    // The asset being received
    pub asset_out: Token,
    // The min amount of asset being received
    pub min_amount_out: Uint128,
    // All the swaps needed for assent_in <> asset_out
    pub swaps: Vec<NextSwapPair>,

    pub recipients: Vec<Recipient>,

    pub partner_fee_amount: Uint128,
    pub partner_fee_recipient: Addr,
}

#[cw_serde]
pub struct NextSwapVlp {
    pub vlp_address: String,
    pub test_fail: Option<bool>,
}

#[cw_serde]
pub struct NextSwapPair {
    pub token_in: Token,
    pub token_out: Token,
    pub test_fail: Option<bool>,
}

#[cw_serde]
pub struct SwapResponse {
    pub amount_out: Uint128,
    pub tx_id: String,
}

#[cw_serde]
pub struct TransferVoucherResponse {
    pub token: Token,
    pub tx_id: String,
}
