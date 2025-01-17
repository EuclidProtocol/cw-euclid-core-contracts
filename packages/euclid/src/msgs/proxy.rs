use schemars::JsonSchema;
use secret_toolkit::utils::{HandleCallback, InitCallback};
use serde::{Deserialize, Serialize};
use crate::token::{Token, TokenType};


#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub escrow_code_id: u64,
    pub escrow_code_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema)]
pub struct InstantiateEscrowMsg {
    // The only allowed Token ID for the contract
    pub token_id: Token,
    // Possibly add allowed denoms in Instantiation
    pub allowed_denom: Option<TokenType>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    InitializeEscrow {
        token_id: Token,
        allowed_denom: Option<TokenType>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {}

impl HandleCallback for ExecuteMsg {
    const BLOCK_SIZE: usize = 256;
}

impl InitCallback for InstantiateEscrowMsg {
    const BLOCK_SIZE: usize = 256;
}