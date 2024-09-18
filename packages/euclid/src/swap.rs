use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, IbcTimeout, Uint128};

use crate::{
    chain::CrossChainUserWithLimit,
    token::{Token, TokenWithDenom},
};

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
    pub swaps: Vec<NextSwapPair>,
    // The timeout specified for the swap
    pub timeout: IbcTimeout,

    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,

    pub partner_fee_amount: Uint128,
    pub partner_fee_recipient: Option<Addr>,
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
pub struct WithdrawResponse {
    pub token: Token,
    pub tx_id: String,
}

#[cw_serde]
pub struct TransferResponse {
    pub token: Token,
    pub tx_id: String,
}
