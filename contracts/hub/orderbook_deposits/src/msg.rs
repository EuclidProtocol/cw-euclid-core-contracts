use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub virtual_balance: String,
    pub admin: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Deposit { token_id: String, amount: Uint128 },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StateResponse)]
    State {},
    #[returns(AssetDepositResponse)]
    AssetDeposit { token_id: String },
    #[returns(UserDepositResponse)]
    UserDeposit { user: String, token_id: String },
    #[returns(WhitelistResponse)]
    Whitelist { token_id: String },
}

#[cw_serde]
pub struct StateResponse {
    pub admin: String,
    pub status: String,
    pub virtual_balance: String,
}

#[cw_serde]
pub struct AssetDepositResponse {
    pub token_id: String,
    pub amount: Uint128,
}

#[cw_serde]
pub struct UserDepositResponse {
    pub user: String,
    pub token_id: String,
    pub amount: Uint128,
}

#[cw_serde]
pub struct WhitelistResponse {
    pub token_id: String,
    pub whitelisted: bool,
}
