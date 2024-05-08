use cosmwasm_schema::cw_serde;
use cosmwasm_std::{IbcTimeout, Uint128};

use crate::token::TokenInfo;

// Struct that stores a certain swap info
#[cw_serde]
pub struct SwapInfo {
    // The asset being swappet
    pub asset: TokenInfo,
    // The amount of asset being swapped
    pub asset_amount: Uint128,
    // The timeout specified for the swap
    pub timeout: IbcTimeout,
}