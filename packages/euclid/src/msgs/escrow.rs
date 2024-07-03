use crate::token::{Token, TokenType};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;

#[cw_serde]
pub struct InstantiateMsg {
    // The only allowed Token ID for the contract
    pub token_id: Token,
    // Possibly add allowed denoms in Instantiation
    pub allowed_denom: Option<TokenType>,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Updates allowed denoms
    AddAllowedDenom { denom: TokenType },
    // Removes a denom from allowed denoms
    DisallowDenom { denom: TokenType },
    DepositNative {},
    // ReleaseTokens { recipient: Addr, amount: Uint128 },

    // Recieve CW20 TOKENS structure
    Receive(Cw20ReceiveMsg),

    // Have a separate Msg for cw20 tokens? flow should be better if the message is unified
    Withdraw { recipient: Addr, amount: Uint128 },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // New escrow queries
    #[returns(TokenIdResponse)]
    TokenId {},

    // New escrow queries
    #[returns(AllowedTokenResponse)]
    TokenAllowed { denom: TokenType },
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct TokenIdResponse {
    pub token_id: String,
}

#[cw_serde]
pub struct AllowedTokenResponse {
    pub allowed: bool,
}

#[cw_serde]
pub struct AmountAndType {
    pub amount: Uint128,
    pub is_native: bool,
}

#[cw_serde]
pub struct EscrowInstantiateResponse {
    pub token: Token,
    pub address: String,
}
