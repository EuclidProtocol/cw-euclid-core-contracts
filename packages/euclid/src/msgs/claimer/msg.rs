use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};

use crate::{
    cross_chain_user::CrossChainUser, msgs::hook::VoucherReceive, recipient::Recipient,
    token::Token,
};

#[cw_serde]
pub struct InstantiateMsg {
    pub router_contract: Addr,
    pub vcoin_address: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ClaimVoucher(SignedTransaction),
    VoucherReceive(VoucherReceive),
    UpdateAdmin(UpdateAdminMsg),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(State)]
    GetState {},
    #[returns(Addr)]
    GetAdmin {},
    #[returns(Vec<(u128, Claim)>)]
    GetSenderClaims {
        sender: CrossChainUser,
        limit: u64,
        offset: u64,
    },
    #[returns(Vec<(u128, Claim)>)]
    GetClaimsByClaimerPubkey {
        pub_key: Binary,
        limit: u64,
        offset: u64,
    },
    #[returns(Claim)]
    GetClaim { claim_id: u128 },
    #[returns(Vec<(u128, Claim)>)]
    GetClaimsByGroupId {
        group_id: String,
        limit: u64,
        offset: u64,
    },
    #[returns(Vec<(u128, Claim)>)]
    GetUserClaims {
        pub_key: Binary,
        limit: u64,
        offset: u64,
    },
    #[returns((u128, Claim))]
    GetClaimByPseudoClaimId { pseudo_claim_id: String },
}

#[cw_serde]
pub struct State {
    pub vcoin_address: Addr,
    pub router_contract: Addr,
}

#[cw_serde]
pub struct SignedTransaction {
    pub data: String,
    pub signature: Binary,
}

#[cw_serde]
pub struct ClaimVoucherData {
    pub claim_id: u128,
    pub recipients: Vec<Recipient>,
}

#[cw_serde]
pub struct UpdateAdminMsg {
    pub new_admin: Addr,
}

#[cw_serde]
pub struct Claim {
    pub token: Token,
    pub amount: Uint128,
    pub claimer_pubkey: Binary,
    pub sender: CrossChainUser,
    pub pseudo_claim_id: Option<String>, // Used by indexers
    pub claim_group_id: Option<String>, // Used by indexers and on chain search using unique identifier
}

#[cw_serde]
pub struct MigrateMsg {}
