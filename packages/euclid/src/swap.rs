use cosmwasm_schema::cw_serde;
use cosmwasm_std::{IbcTimeout, Uint128};

use crate::token::TokenInfo;

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
    let sender: Vec<&str> = swap_id.split("-").collect();
    sender[0].to_string()
}