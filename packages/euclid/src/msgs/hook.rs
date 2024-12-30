use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary};

use crate::error::ContractError;

#[cw_serde]
pub enum EuclidReceive {
    ForwardSwap(EuclidForwardSwap),
}

#[cw_serde]
pub struct EuclidForwardSwap {
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
    pub fn to_cosmos_msg(&self) -> Result<Binary, ContractError> {
        Ok(to_json_binary(&EuclidReceiverMsg::EuclidReceive(
            self.clone(),
        ))?)
    }
}
