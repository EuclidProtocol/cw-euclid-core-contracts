use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128};

use crate::{
    chain::{ChainUid, CrossChainUser},
    token::Token,
};

#[cw_serde]
pub struct InstantiateMsg {
    pub factory_address: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ClaimVoucher(SignedTransaction),
    CreateVoucherClaim(CreateVoucherClaim),
    UpdateAdmin(UpdateAdminMsg),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(State)]
    GetState {},
    #[returns(Vec<u128>)]
    GetSenderClaims { sender: String },
    #[returns(Vec<u128>)]
    GetUserClaims { pub_key: Binary },
    #[returns(Claim)]
    GetClaim { claim_id: u128 },
}

#[cw_serde]
pub struct State {
    // Address of the virtual balance contract
    pub factory_address: Addr,
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
}

#[cw_serde]
pub struct CreateVoucherClaim {
    pub token: Token,
    pub amount: Uint128,
    pub claimer_pubkey: Binary,
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
    pub sender: String,
}

#[cw_serde]
pub struct MigrateMsg {}
