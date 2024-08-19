use cosmwasm_std::{from_json, to_json_binary, DepsMut, Reply, Response, SubMsgResult};
use cw0::parse_reply_execute_data;
use euclid::{error::ContractError, msgs::vlp::VlpSwapResponse};

pub const virtual_balance_TRANSFER_REPLY_ID: u64 = 1;
pub const NEXT_SWAP_REPLY_ID: u64 = 2;

pub fn on_next_swap_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let swap_response: VlpSwapResponse = from_json(execute_data.data.unwrap_or_default())?;

            Ok(Response::new()
                .add_attribute("action", "reply_next_swap")
                .add_attribute("swap_id", swap_response.tx_id.clone())
                .add_attribute("swap_response", format!("{swap_response:?}"))
                .set_data(to_json_binary(&swap_response)?))
        }
    }
}

pub fn on_virtual_balance_transfer_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => Ok(Response::new().add_attribute("action", "virtual_balance_transfer")),
    }
}
