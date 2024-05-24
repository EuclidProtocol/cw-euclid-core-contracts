use crate::msg::AcknowledgementMsg;
use cosmwasm_std::{to_json_binary, Binary};
use euclid::error::ContractError;

pub fn make_ack_success() -> Result<Binary, ContractError> {
    let res = AcknowledgementMsg::Ok(b"1");
    Ok(to_json_binary(&res)?)
}

pub fn make_ack_fail(err: String) -> Result<Binary, ContractError> {
    let res = AcknowledgementMsg::Error::<()>(err);
    Ok(to_json_binary(&res)?)
}
