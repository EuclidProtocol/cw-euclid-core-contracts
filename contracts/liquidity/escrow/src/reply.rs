use crate::state::REFUND_ADDRESS;
use cosmwasm_std::{DepsMut, Reply, Response, SubMsgResult};
use euclid::error::ContractError;

pub const FORWARDING_MESSAGE_REPLY_ID: u64 = 1;

pub fn handle_refund(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            //TODO keep refund address?
            Ok(Response::new()
                .add_attribute("action", "forwarding_message")
                .add_attribute("error", err))
        }
        SubMsgResult::Ok(_res) => {
            REFUND_ADDRESS.remove(deps.storage);
            Ok(Response::new().add_attribute("action", "forwarding_message"))
        }
    }
}
