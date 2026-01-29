use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};

#[cw_serde]
pub struct InstantiateMsg {
    pub message_signer: Validator,
    pub signature_threshold: u8,
    pub validators: Vec<Validator>,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ExecuteMetaTransaction(MetaTransaction),
    UpdateState(UpdateStateMsg),
    UpdateAdmin(UpdateAdminMsg),
    AddValidator { validator: Validator },
    RemoveValidator { validator: Validator },
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(State)]
    GetState {},

    #[returns(bool)]
    NonceRelayed { nonce: String },

    #[returns(ValidatorsResponse)]
    Validators {},
}

#[cw_serde]
pub struct Validator {
    pub pubkey: Binary,
    pub address: String,
}

#[cw_serde]
pub struct State {
    pub message_signer: Validator,
    pub signature_threshold: u8,
    pub admin: Addr,
}

#[cw_serde]
pub struct MetaTransaction {
    pub data: String,
    pub admin_signature: Binary,
    pub validator_signatures: Vec<Binary>,
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
    pub message_signer: Option<Validator>,
    pub signature_threshold: Option<u8>,
}

#[cw_serde]
pub struct UpdateAdminMsg {
    pub new_admin: Addr,
}

#[cw_serde]
pub struct ValidatorsResponse {
    pub validators: Vec<Validator>,
}

#[cw_serde]
pub struct MigrateMsg {}
