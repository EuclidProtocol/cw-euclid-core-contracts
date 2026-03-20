use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Uint256};

use crate::msgs::orderbook_deposits::AssetTotal;
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
    #[returns(RootResponse)]
    CurrentRoot {},
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
    pub amount: Uint256,
}

#[cw_serde]
pub struct UserDepositResponse {
    pub user: String,
    pub token_id: String,
    pub amount: Uint256,
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
pub struct RootResponse {
    pub root_id: String,
    pub root_hash: Binary,
    pub per_asset_totals: Vec<AssetTotal>,
    pub da_hash: Option<Binary>,
    pub da_url: Option<String>,
    pub proposed_at: u64,
}
