use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};

#[cw_serde]
pub struct InstantiateMsg {
    pub router_contract: Addr,
    pub relayer_pubkey: Binary,
    pub relayer_address: String,
    pub authorized_addresses: Vec<Addr>,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ExecuteMetaTransaction(MetaTransaction),
    ExecuteAuthorizedTransaction(AuthorizedTransaction),
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
    // Public key of the off chain relayer wallet used to sign meta transactions
    pub relayer_pubkey: Binary,
    // Address of the on chain relayer wallet used to sign authorized transactions
    pub relayer_address: String,
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
    pub call_data: Binary,
    pub expiry: u64,
    pub nonce: String,
}

#[cw_serde]
pub struct AuthorizedTransaction {
    pub call_data: Binary,
    pub nonce: String,
}

#[cw_serde]
pub struct UpdateStateMsg {
    pub relayer_pubkey: Option<Binary>,
    pub relayer_address: Option<String>,
    pub authorized_addresses: Option<Vec<Addr>>,
}

#[cw_serde]
pub struct UpdateAdminMsg {
    pub new_admin: Addr,
}

#[cw_serde]
pub struct MigrateMsg {}
