use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};

use crate::{
    chain::{ChainUid, CrossChainUser},
    msgs::hook::EuclidReceive,
    token::Token,
};

use super::hook::VirtualBalanceReceive;

#[cw_serde]
pub struct InstantiateMsg {
    pub factory_address: Addr,
    pub vcoin_address: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ClaimVoucher(SignedTransaction),
    VirtualBalanceReceive(VirtualBalanceReceive),
    UpdateAdmin(UpdateAdminMsg),
}

#[cw_serde]
pub enum VirtualBalanceReceiveHookMsg {
    CreateVoucherClaim(CreateVoucherClaim),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(State)]
    GetState {},
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
    #[returns(Claim)]
    GetClaimByPseudoClaimId { pseudo_claim_id: String },
}

#[cw_serde]
pub struct State {
    pub factory_address: Addr,
    pub vcoin_address: Addr,
    pub chain_uid: ChainUid,
    pub admin: Addr,
}

#[cw_serde]
pub struct SignedTransaction {
    pub data: String,
    pub signature: Binary,
}

#[cw_serde]
pub struct ClaimVoucherData {
    pub claim_id: u128,
    pub recipient: CrossChainUser,
    pub release_funds: bool,
    pub release_msg: Option<EuclidReceive>,
}

#[cw_serde]
pub struct CreateVoucherClaim {
    pub claimer_pubkey: Binary,
    // Adds a pseudo claim id used by indexers
    pub pseudo_claim_id: Option<String>,
    // Group id used by indexers and on chain search using unique identifier
    pub claim_group_id: Option<String>,
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
