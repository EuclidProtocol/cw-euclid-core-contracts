use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, IbcTimeout, Uint128, Uint256};

use crate::{
    chain::CrossChainUserWithLimit,
    error::ContractError,
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
    // The timeout specified for the swap
    pub timeout: IbcTimeout,

    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,

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
pub struct WithdrawResponse {
    pub token: Token,
    pub tx_id: String,
}

#[cw_serde]
pub struct TransferResponse {
    pub token: Token,
    pub tx_id: String,
}

// Function to calculate the asset to be recieved after a swap
pub fn calculate_swap(
    swap_amount: Uint128,
    reserve_in: Uint128,
    reserve_out: Uint128,
) -> Result<Uint128, ContractError> {
    let reserve_in = Uint256::from(reserve_in);
    let reserve_out = Uint256::from(reserve_out);
    // Calculate the k constant product
    let k = reserve_in.checked_mul(reserve_out)?;
    // Calculate the new reserve of token 1
    let new_reserve_in = reserve_in.checked_add(swap_amount.into())?;
    // Calculate the new reserve of token 2
    let new_reserve_out = k.checked_div(new_reserve_in)?;
    // Calculate the amount of token 2 to be recieved
    let token_2_recieved = reserve_out.checked_sub(new_reserve_out)?;
    let token_2_recieved =
        Uint128::try_from(token_2_recieved).map_err(|_| ContractError::new("Overflow"))?;

    Ok(token_2_recieved)
}
