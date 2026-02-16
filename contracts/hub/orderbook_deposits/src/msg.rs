use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Uint128};
use euclid::msgs::hook::VoucherReceive;

use crate::state::{AssetTotal, OrderbookDepositsStatus};

#[cw_serde]
pub struct InstantiateMsg {
    pub virtual_balance: String,
    pub admin: Option<String>,
    pub root_challenge_period: Option<u64>,
    pub permit_signer_pubkey: Option<Binary>,
    pub permit_signer_address: Option<String>,
    pub authorized_posters: Option<Vec<String>>,
}

#[cw_serde]
pub enum ExecuteMsg {
    SetWhitelist {
        token_id: String,
        whitelisted: bool,
    },
    VoucherReceive(VoucherReceive),
    UpdateConfig {
        admin: Option<String>,
        status: Option<OrderbookDepositsStatus>,
        root_challenge_period: Option<u64>,
        permit_signer_pubkey: Option<Binary>,
        permit_signer_address: Option<String>,
        authorized_posters: Option<Vec<String>>,
    },
    ProposeRoot {
        root_id: String,
        root_hash: Binary,
        per_asset_totals: Vec<AssetTotal>,
        da_hash: Option<Binary>,
        da_url: Option<String>,
    },
    ActivateRoot {
        root_id: String,
    },
    Withdraw {
        root_id: String,
        amount: Uint128,
        nonce: u64,
        leaf: WithdrawalLeaf,
        proof: Vec<MerkleProofStep>,
        permit: Permit,
        destination_chain_uid: String,
        destination: String,
    },
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
pub struct RootResponse {
    pub root_id: String,
    pub root_hash: Binary,
    pub per_asset_totals: Vec<AssetTotal>,
    pub da_hash: Option<Binary>,
    pub da_url: Option<String>,
    pub proposed_at: u64,
}

#[cw_serde]
pub struct WithdrawalLeaf {
    pub user: String,
    pub token_id: String,
    pub balance: Uint128,
}

#[cw_serde]
pub enum ProofPosition {
    Left,
    Right,
}

#[cw_serde]
pub struct MerkleProofStep {
    pub hash: Binary,
    pub position: ProofPosition,
}

#[cw_serde]
pub struct Permit {
    pub data: String,
    pub signature: Binary,
}

#[cw_serde]
pub struct PermitData {
    pub root_id: String,
    pub user: String,
    pub token_id: String,
    pub amount: Uint128,
    pub nonce: u64,
    pub destination_chain_uid: String,
    pub destination: String,
    pub expiry: u64,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub enum VirtualBalanceReceiveHookMsg {
    Deposit {},
}
