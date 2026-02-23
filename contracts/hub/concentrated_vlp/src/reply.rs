use cosmwasm_std::{from_json, to_json_binary, DepsMut, Reply, Response, SubMsgResult};
use cw_utils::parse_execute_response_data;
use euclid::{error::ContractError, msgs::vlp::base::VlpSwapResponse};
use function_name::named;

#[named]
pub fn on_next_swap_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
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
