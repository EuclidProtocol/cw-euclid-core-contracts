use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::{chain::CrossChainUser, token::Token};

#[cw_serde]
pub struct EscrowReleaseRequest {
    pub sender: CrossChainUser,
    pub tx_id: String,

    pub token: Token,
    pub amount: Uint128,
    pub to_address: String,
}

// Struct to handle Acknowledgement Response for a Liquidity Request
#[cw_serde]
pub struct EscrowReleaseResponse {
    pub success: bool,
}
