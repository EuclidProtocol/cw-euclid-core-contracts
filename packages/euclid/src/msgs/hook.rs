use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary, Uint128};

use crate::{error::ContractError, token::TokenType};

#[cw_serde]
pub enum EuclidReceive {
    ForwardSwap(EuclidForwardSwap),
}

#[cw_serde]
pub struct EuclidForwardSwap {
    pub data: Binary,
    pub to_token: TokenType,
    pub minimum_receive: Uint128,
    pub recipient: String,
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
