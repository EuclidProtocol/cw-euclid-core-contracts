use schemars::JsonSchema;
use secret_toolkit::utils::{HandleCallback, InitCallback};
use serde::{Deserialize, Serialize};
use crate::token::{Token, TokenType};


#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub escrow_code_id: u64,
    pub escrow_code_hash: String,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
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