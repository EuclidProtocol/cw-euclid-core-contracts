use cosmwasm_std::{ensure, DepsMut, Env, Reply, Response, SubMsgResult};
use forwarding::msgs::common_old::EuclidReceive;
use forwarding::msgs::errors_old::ContractError;

use crate::state::{ForwardingState, FORWARDING_STATE};

pub const ASTRO_SWAP_REPLY_ID: u64 = 1;

pub fn on_astro_swap_reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::new(&format!(
            "Astroport swap failed: {}",
            err
        ))),
        SubMsgResult::Ok(..) => {
            let ForwardingState {
                from_token,
                from_amount,
                previous_balance,
                swap_msg,
            } = FORWARDING_STATE.load(deps.storage)?;

            FORWARDING_STATE.remove(deps.storage);

            let new_balance = swap_msg
                .to_token
                .get_balance(deps.as_ref(), env.contract.address.to_string())?;

            let swap_amount = new_balance.checked_sub(previous_balance)?;

            ensure!(
                swap_amount >= swap_msg.minimum_receive,
                ContractError::MinReceived {
                    expected: swap_msg.minimum_receive,
                    received: swap_amount,
                }
            );

            let forwading_msg = match swap_msg.forwarding_msg {
                Some(forwarding_msg) => {
                    Some(EuclidReceive::from_msg(&forwarding_msg).to_receiver_msg()?)
                }
                None => None,
            };

            let transfer_msg = swap_msg.to_token.create_transfer_msg(
                swap_amount,
                swap_msg.recipient,
                None,
                forwading_msg,
            )?;

            Ok(Response::new()
                .add_attribute("action", "reply_astro_swap")
                .add_attribute("dex", "astroport")
                .add_attribute("complete_swap_in_amount", from_amount)
                .add_attribute("complete_swap_in_token", from_token.get_key())
                .add_attribute("complete_swap_out_amount", swap_amount)
                .add_attribute("complete_swap_out_token", swap_msg.to_token.get_key())
                .add_message(transfer_msg))
        }
    }
}
