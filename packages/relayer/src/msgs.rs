use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary};
use euclid::admin::{AdminType, EuclidAdmin};
use euclid::chain::ChainUid;

#[cw_serde]
pub struct InstantiateMsg {
    pub message_signer: Validator,
    pub signature_threshold: u8,
}

#[cw_serde]
#[derive(cw_orch::ExecuteFns)]
pub enum ExecuteMsg {
    ExecuteMetaTransaction(MetaTransaction),
    UpdateState(UpdateStateMsg),
    UpdateAdmin(UpdateAdminMsg),
    AddValidator {
        validator: Validator,
        chain_uid: ChainUid,
    },
    RemoveValidator {
        validator: Validator,
        chain_uid: ChainUid,
    },
}

#[cw_serde]
#[derive(cw_orch::QueryFns, QueryResponses)]
pub enum QueryMsg {
    #[returns(State)]
    GetState {},

    #[returns(EuclidAdmin)]
    GetAdmin {},

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
}

#[cw_serde]
pub struct ValidatorSignature {
    pub pubkey: Binary,
    pub signature: Binary,
    pub expiry: u64,
}

#[cw_serde]
pub struct MetaTransaction {
    pub data: String,
    pub expiry: u64,
    pub admin_signature: Binary,
    pub validator_signatures: Vec<ValidatorSignature>,
    pub chain_uid: ChainUid,
}

#[cw_serde]
pub struct MetaTransactionData {
    pub target: Addr,
    pub call_data: Binary,
    pub nonce: String,
}

#[cw_serde]
pub struct UpdateStateMsg {
    pub message_signer: Option<Validator>,
    pub signature_threshold: Option<u8>,
}

#[cw_serde]
pub struct UpdateAdminMsg {
    pub new_admin: String,
    pub admin_type: AdminType,
}

#[cw_serde]
pub struct ValidatorsResponseItem {
    pub validator: Validator,
    pub chain_uid: ChainUid,
}

#[cw_serde]
pub struct ValidatorsResponse {
    pub validators: Vec<ValidatorsResponseItem>,
}

#[cw_serde]
pub struct MigrateMsg {}
