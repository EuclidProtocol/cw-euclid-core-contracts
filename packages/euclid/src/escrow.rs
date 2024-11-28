use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::Uint128;

use crate::{chain::CrossChainUser, token::Token};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowReleaseRequest {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub token: Token,
    pub amount: Uint128,
    pub to_address: String,
}

// Struct to handle Acknowledgement Response for a Liquidity Request
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowReleaseResponse {
    pub success: bool,
}
