use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary, Uint128};

use crate::{chain::CrossChainUser, error::ContractError};

#[cw_serde]
pub struct EuclidReceive {
    pub data: Binary,
    // Metadata to be logged into events for some off chain oracle/analytics
    pub meta: Option<String>,
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
}

#[cw_serde]
pub struct VirtualBalanceReceive {
    pub sender: CrossChainUser,
    pub amount: Uint128,
    pub token_id: String,
    pub msg: Binary,
}

// This is just a helper to properly serialize the above message
#[cw_serde]
pub enum VirtualBalanceReceiverMsg {
    VirtualBalanceReceive(VirtualBalanceReceive),
}

impl VirtualBalanceReceive {
    pub fn to_receiver_msg(&self) -> Result<Binary, ContractError> {
        Ok(to_json_binary(
            &VirtualBalanceReceiverMsg::VirtualBalanceReceive(self.clone()),
        )?)
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
