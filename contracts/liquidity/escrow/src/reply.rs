use crate::state::{REFUND_ADDRESS, REFUND_ASSETS};
use cosmwasm_std::{DepsMut, Reply, Response, SubMsgResult};
use euclid::error::ContractError;

pub const FORWARDING_MESSAGE_REPLY_ID: u64 = 1;

pub fn handle_refund(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    // Get the first refund asset and pop it from the list
    let refund_assets = REFUND_ASSETS.load(deps.storage).unwrap_or_default();

    // Remove the first refund asset from the list
    let (current_refund, remaining_refund_assets) = refund_assets
        .split_first()
        .ok_or(ContractError::new("Didn't find any refund assets"))?;
    REFUND_ASSETS.save(deps.storage, &remaining_refund_assets.to_vec())?;

    let refund_address = REFUND_ADDRESS.may_load(deps.storage)?;

    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            if let Some(refund_address) = refund_address {
                let refund_msg = current_refund.0.create_transfer_msg(
                    current_refund.1,
                    refund_address,
                    None,
                    None,
                )?;
                Ok(Response::new()
                    .add_message(refund_msg)
                    .add_attribute("action", "forwarding_message")
                    .add_attribute("error", err))
            } else {
                Err(ContractError::new(&format!(
                    "No refund address found: Forward message failed with error: {}",
                    err
                )))
            }
        }
        SubMsgResult::Ok(_res) => Ok(Response::new().add_attribute("action", "forwarding_message")),
    }
}
