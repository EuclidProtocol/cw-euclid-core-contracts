use cosmwasm_std::{to_json_binary, Binary};
use euclid_ibc::msg::AcknowledgementMsg;

pub fn make_ack_success() -> Binary {
    let res = AcknowledgementMsg::Ok(b"1");
    to_json_binary(&res).unwrap()
}

pub fn make_ack_fail(err: String) -> Binary {
    let res = AcknowledgementMsg::Error::<()>(err);
    to_json_binary(&res).unwrap()
}
