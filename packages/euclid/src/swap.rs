use cosmwasm_schema::cw_serde;
use cosmwasm_std::{IbcTimeout, Uint128};

use crate::token::{Token, TokenInfo};

// Struct that stores a certain swap info
#[cw_serde]
pub struct SwapInfo {
    // The asset being swappet
    pub asset: TokenInfo,
    // The asset being received
    pub asset_out: TokenInfo,
    // The amount of asset being swapped
    pub asset_amount: Uint128,
    // The timeout specified for the swap
    pub timeout: IbcTimeout,
    // The Swap Main Identifier
    pub swap_id: String,
}

// Function to extract sender from swap_id
pub fn extract_sender(swap_id: &str) -> String {
    let sender: Vec<&str> = swap_id.split('-').collect();
    sender[0].to_string()
}

#[cw_serde]
pub struct LiquidityTxInfo {
    pub sender: String,
    pub token_1_liquidity: Uint128,
    pub token_2_liquidity: Uint128,
    pub liquidity_id: String,
}

// Function to extract sender from liquidity_id
pub fn extract_sender_liquidity(liquidity_id: &str) -> String {
    let sender: Vec<&str> = liquidity_id.split('-').collect();
    sender[0].to_string()
}

#[cw_serde]
pub struct SwapResponse {
    pub asset: Token,
    pub asset_out: Token,
    pub asset_amount: Uint128,
    pub amount_out: Uint128,
    // Add Swap Unique Identifier
    pub swap_id: String,
}
