use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};

use crate::chain::ChainUid;

#[cw_serde]
pub struct InstantiateMsg {
    pub router_contract: Addr,
    pub authorized_addresses: Vec<Addr>,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ExecuteMetaTransaction(MetaTransaction),
    UpdateState(UpdateStateMsg),
    UpdateAdmin(UpdateAdminMsg),
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(State)]
    GetState {},

    #[returns(bool)]
    NonceRelayed {
        chain_uid: ChainUid,
        address: String,
        nonce: String,
    },
}

#[cw_serde]
pub struct State {
    pub router_contract: Addr,
    pub admin: Addr,
}

#[cw_serde]
pub struct MetaTransaction {
    pub data: String,
    pub signature: Binary,
}

#[cw_serde]
pub struct MetaTransactionData {
    pub signer_address_src_chain: String,
    pub chain_uid_src_chain: ChainUid,
    pub pubkey_singer: Binary,
    pub call_data: Binary,
    pub expiry: u64,
    pub nonce: String,
}

#[cw_serde]
pub struct UpdateStateMsg {
    pub authorized_addresses: Option<Vec<Addr>>,
}

#[cw_serde]
pub struct UpdateAdminMsg {
    pub new_admin: Addr,
}

#[cw_serde]
pub struct MigrateMsg {}
