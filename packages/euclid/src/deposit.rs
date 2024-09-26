use crate::{
    chain::{CrossChainUser, CrossChainUserWithLimit},
    token::{Token, TokenWithDenom},
};
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
    pub recipient: Option<CrossChainUserWithLimit>,
}

// Struct to handle Acknowledgement Response for a Deposit Token Request
#[cw_serde]
pub struct DepositTokenResponse {
    pub amount: Uint128,
    pub token: Token,
    pub sender: CrossChainUser,
}
