use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_json_binary, Binary};
use euclid::error::ContractError;

/// A custom acknowledgement type.
/// The success type `T` depends on the PacketMsg variant.
///
/// This could be refactored to use [StdAck] at some point. However,
/// it has a different success variant name ("ok" vs. "result") and
/// a JSON payload instead of a binary payload.
///
/// [StdAck]: https://github.com/CosmWasm/cosmwasm/issues/1512
#[cw_serde]
pub enum AcknowledgementMsg<S> {
    Ok(S),
    Error(String),
}

impl<S> AcknowledgementMsg<S> {
    pub fn unwrap(self) -> Result<S, ContractError> {
        match self {
            AcknowledgementMsg::Ok(data) => Ok(data),
            AcknowledgementMsg::Error(err) => Err(ContractError::new(&err)),
        }
    }

    pub fn unwrap_err(self) -> Result<String, ContractError> {
        match self {
            AcknowledgementMsg::Ok(_) => Err(ContractError::new("Not an error")),
            AcknowledgementMsg::Error(err) => Ok(err),
        }
    }
}

pub fn make_ack_success() -> Result<Binary, ContractError> {
    let res = AcknowledgementMsg::Ok(b"1");
    Ok(to_json_binary(&res)?)
}

pub fn make_ack_fail(err: String) -> Result<Binary, ContractError> {
    let res = AcknowledgementMsg::Error::<()>(err);
    Ok(to_json_binary(&res)?)
}
