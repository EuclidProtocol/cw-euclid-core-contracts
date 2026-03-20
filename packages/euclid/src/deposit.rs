use crate::{
    cross_chain_user::CrossChainUser,
    token::{Token, TokenWithDenom},
};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;

#[cw_serde]
pub struct DepositTokenRequest {
    pub sender: String,
    pub tx_id: String,
    // The asset being swapped
    pub asset_in: TokenWithDenom,
    // The amount of asset being swapped
    pub amount_in: Uint256,
}

// Struct to handle Acknowledgement Response for a Deposit Token Request
#[cw_serde]
pub struct DepositTokenResponse {
    pub amount: Uint256,
    pub token: Token,
    pub sender: CrossChainUser,
}
