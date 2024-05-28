use cosmwasm_schema::cw_serde;
use cosmwasm_std::{IbcTimeout, Uint128};

use crate::{
    error::ContractError,
    token::{Token, TokenInfo},
};

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

#[cw_serde]
pub struct SwapResponse {
    pub asset: Token,
    pub asset_out: Token,
    pub asset_amount: Uint128,
    pub amount_out: Uint128,
    // Add Swap Unique Identifier
    pub swap_id: String,
}

pub fn generate_id(sender: &str, count: u128) -> String {
    format!("{sender}-{count}")
}

#[cw_serde]
pub struct SwapExtractedId {
    pub sender: String,
    pub index: u128,
}

// Function to extract sender from swap_id
pub fn parse_swap_id(id: &str) -> Result<SwapExtractedId, ContractError> {
    let parsed: Vec<&str> = id.split('-').collect();
    Ok(SwapExtractedId {
        sender: parsed[0].to_string(),
        index: parsed[1].parse()?,
    })
}
