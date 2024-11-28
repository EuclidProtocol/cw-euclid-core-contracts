use crate::token::{Pair, Token, TokenType};
use cosmwasm_schema::QueryResponses;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Addr, Binary, Uint128};
use secret_toolkit::utils::InitCallback;
use snip20_reference_impl::receiver::Snip20ReceiveMsg;

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {
    // The only allowed Token ID for the contract
    pub token_id: Token,
    // Possibly add allowed denoms in Instantiation
    pub allowed_denom: Option<TokenType>,
}

impl InitCallback for InstantiateMsg {
    const BLOCK_SIZE: usize = 256;
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
// #[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    // Updates allowed denoms
    AddAllowedDenom {
        denom: TokenType,
    },
    // Removes a denom from allowed denoms
    DisallowDenom {
        denom: TokenType,
    },
    DepositNative {},
    // Recieve SNIP20 TOKENS structure
    Receive(Snip20ReceiveMsg),

    // Have a separate Msg for snip20 tokens? flow should be better if the message is unified
    Withdraw {
        recipient: Addr,
        amount: Uint128,
        memo: Option<String>,
        decoys: Option<Vec<Addr>>,
        entropy: Option<Binary>,
        padding: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StateResponse)]
    State {},

    // New escrow queries
    #[returns(TokenIdResponse)]
    TokenId {},

    // New escrow queries
    #[returns(AllowedTokenResponse)]
    TokenAllowed { denom: TokenType },

    #[returns(AllowedDenomsResponse)]
    AllowedDenoms {},
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct StateResponse {
    pub token: Token,
    pub factory_address: Addr,
    pub total_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct TokenIdResponse {
    pub token_id: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllowedDenomsResponse {
    pub denoms: Vec<TokenType>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct AllowedTokenResponse {
    pub allowed: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct EscrowInstantiateResponse {
    pub token: Token,
    pub address: String,
    pub code_hash: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct Snip20InstantiateResponse {
    pub pair: Pair,
    pub address: String,
    pub code_hash: String,
    pub vlp: String,
}
