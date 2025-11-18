use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use crate::chain::{ChainUid, CrossChainUser};

#[cw_serde]
pub struct InstantiateMsg {
    pub router_contract: Addr,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ExecuteMetaTransaction(MetaTransaction),
    UpdateAdmin(UpdateAdminMsg),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(State)]
    GetState {},

    #[returns(NonceRelayedResponse)]
    NonceRelayed { nonce: String },
}

#[cw_serde]
pub struct State {
    pub router_contract: Addr,
    pub admin: Addr,
}

#[cw_serde]
pub struct UpdateAdminMsg {
    pub new_admin: Addr,
}

#[cw_serde]
pub struct MetaTransaction {
    pub data: MetaTransactionData,
    pub signature: String,     // Can be hex or base64 based on chain type
    pub signer_pubkey: String, // Can be hex or base64 based on chain type
}

#[cw_serde]
pub struct MetaTransactionData {
    pub signer_address: String,
    pub signer_prefix: String, // bech32 for cosmos and 0x for evm
    pub signer_chain_uid: ChainUid,
    pub call_data: Vec<MetaTransactionCallData>,
    pub expiry: u64,
    pub nonce: String,
}

#[cw_serde]
pub struct MetaTransactionCallData {
    pub target: Addr,
    pub call_data: String,
}

#[cw_serde]
pub struct NonceRelayedResponse {
    pub height: Uint128,
}
#[cw_serde]
pub struct MigrateMsg {}
