use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;

#[cw_serde]
pub struct InstantiateMsg {
    pub name: String,
    pub symbol: String,
    pub minter: Addr,
    pub admin: Addr,
}

#[cw_serde]
pub enum ExecuteMsg {
    Mint {
        token_id: String,
        owner: String,
        token_uri: Option<String>,
    },
    Burn {
        token_id: String,
    },
    Transfer {
        token_id: String,
        recipient: String,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(OwnerOfResponse)]
    OwnerOf { token_id: String },
    #[returns(TokenInfoResponse)]
    TokenInfo { token_id: String },
    #[returns(TokensResponse)]
    TokensByOwner { owner: String },
    #[returns(TokensResponse)]
    AllTokens {},
    #[returns(StateResponse)]
    State {},
}

#[cw_serde]
pub struct OwnerOfResponse {
    pub owner: String,
}

#[cw_serde]
pub struct TokenInfoResponse {
    pub token_id: String,
    pub owner: String,
    pub token_uri: Option<String>,
}

#[cw_serde]
pub struct TokensResponse {
    pub tokens: Vec<String>,
}

#[cw_serde]
pub struct StateResponse {
    pub name: String,
    pub symbol: String,
    pub minter: Addr,
    pub admin: Addr,
    pub total_tokens: u64,
}

#[cw_serde]
pub struct MigrateMsg {}
