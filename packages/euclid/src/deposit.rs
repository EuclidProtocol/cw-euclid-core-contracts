use crate::{chain::CrossChainUserWithLimit, token::TokenWithDenom};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{IbcTimeout, Uint128};

#[cw_serde]
pub struct DepositTokenRequest {
    pub sender: String,
    pub tx_id: String,
    // The asset being swapped
    pub asset_in: TokenWithDenom,
    // The amount of asset being swapped
    pub amount_in: Uint128,
    // The timeout specified for the swap
    pub timeout: IbcTimeout,

    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,
}
