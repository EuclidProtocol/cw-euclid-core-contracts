use crate::msgs::hook::VoucherReceive;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Uint128};

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
pub enum OrderbookDepositsStatus {
    Active,
    Paused,
    Stopped,
}

#[cw_serde]
pub struct AssetTotal {
    pub token_id: String,
    pub amount: Uint128,
}

#[cw_serde]
pub struct WithdrawalLeaf {
    pub user: String,
    pub token_id: String,
    pub balance: Uint128,
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
pub enum ProofPosition {
    Left,
    Right,
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
pub enum VirtualBalanceReceiveHookMsg {
    Deposit {},
}
