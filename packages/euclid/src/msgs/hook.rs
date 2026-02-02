use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary, Uint128};

use crate::{cross_chain_user::CrossChainUser, error::ContractError};

#[cw_serde]
pub struct EuclidReceive {
    pub msg: Binary,
}

// This is just a helper to properly serialize the above message
#[cw_serde]
pub enum EuclidReceiverMsg {
    EuclidReceive(EuclidReceive),
}

impl EuclidReceive {
    pub fn to_receiver_msg(&self) -> Result<Binary, ContractError> {
        Ok(to_json_binary(&EuclidReceiverMsg::EuclidReceive(
            self.clone(),
        ))?)
    }

    pub fn from_msg(binary: Binary) -> Self {
        Self { msg: binary }
    }
}

#[cw_serde]
pub struct VoucherReceive {
    pub sender: CrossChainUser,
    pub amount: Uint128,
    pub token_id: String,
    pub msg: Binary,
}

// This is just a helper to properly serialize the above message
#[cw_serde]
pub enum VoucherReceiverMsg {
    VoucherReceive(VoucherReceive),
}

impl VoucherReceive {
    pub fn to_receiver_msg(&self) -> Result<Binary, ContractError> {
        Ok(to_json_binary(&VoucherReceiverMsg::VoucherReceive(
            self.clone(),
        ))?)
    }
}

#[cw_serde]
pub struct MetaReceive {
    pub verified_sender: CrossChainUser,
    pub call_data: String,
}

#[cw_serde]
pub enum MetaReceiverMsg {
    MetaReceive(MetaReceive),
}

impl MetaReceive {
    pub fn to_receiver_msg(&self) -> Result<Binary, ContractError> {
        Ok(to_json_binary(&MetaReceiverMsg::MetaReceive(self.clone()))?)
    }
}

#[cw_serde]
pub struct EuclidAcknowledgement {
    pub ack: Binary,
    pub msg: Binary,
}

// This is just a helper to properly serialize the above message
#[cw_serde]
pub enum EuclidAcknowledgementMsg {
    EuclidAcknowledgement(EuclidAcknowledgement),
}

impl EuclidAcknowledgement {
    pub fn to_receiver_msg(&self) -> Result<Binary, ContractError> {
        Ok(to_json_binary(
            &EuclidAcknowledgementMsg::EuclidAcknowledgement(self.clone()),
        )?)
    }
}
