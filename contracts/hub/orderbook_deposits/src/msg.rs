use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use euclid::msgs::hook::VirtualBalanceReceive;

#[cw_serde]
pub struct InstantiateMsg {
    pub virtual_balance: String,
    pub admin: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    SetWhitelist { token_id: String, whitelisted: bool },
    VirtualBalanceReceive(VirtualBalanceReceive),
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
    #[returns(WhitelistListResponse)]
    WhitelistedAssets {
        start_after: Option<String>,
        limit: Option<u32>,
    },
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

#[cw_serde]
pub struct WhitelistListResponse {
    pub assets: Vec<WhitelistResponse>,
}

#[cw_serde]
pub enum VirtualBalanceReceiveHookMsg {
    Deposit {},
}
