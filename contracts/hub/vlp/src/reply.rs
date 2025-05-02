use cosmwasm_std::{from_json, to_json_binary, DepsMut, Reply, Response, SubMsgResult};
use cw_utils::parse_execute_response_data;
use euclid::{error::ContractError, pool::VlpSwapResponse};

pub const NEXT_SWAP_REPLY_ID: u64 = 1;

pub fn on_next_swap_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            let data = result.msg_responses[0].value.as_slice();

            let execute_data =
                parse_execute_response_data(data).map_err(|res| ContractError::Generic {
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
