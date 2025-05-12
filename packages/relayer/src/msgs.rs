use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};

#[cw_serde]
pub struct InstantiateMsg {
    pub relayer_pubkey: Binary,
    pub relayer_address: String,
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
    NonceRelayed { nonce: String },
}

#[cw_serde]
pub struct State {
    pub relayer_pubkey: Binary,
    pub relayer_address: String,
    pub admin: Addr,
}

#[cw_serde]
pub struct MetaTransaction {
    pub data: String,
    pub signature: Binary,
}

#[cw_serde]
pub struct MetaTransactionData {
    pub target: Addr,
    pub call_data: Binary,
    pub expiry: u64,
    pub nonce: String,
}

#[cw_serde]
pub struct UpdateStateMsg {
    pub relayer_pubkey: Option<Binary>,
    pub relayer_address: Option<String>,
}

#[cw_serde]
pub struct UpdateAdminMsg {
    pub new_admin: Addr,
}

#[cw_serde]
pub struct MigrateMsg {}
