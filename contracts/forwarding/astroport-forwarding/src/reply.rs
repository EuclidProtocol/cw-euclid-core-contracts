use cosmwasm_std::{
    ensure, from_json, to_json_binary, DepsMut, Env, Reply, Response, SubMsgResult,
};
use euclid::{error::ContractError, msgs::hook::EuclidReceiverMsg};
use forwarding::msgs::astroport::SwapMsg;

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
                msg,
            } = FORWARDING_STATE.load(deps.storage)?;
            let swap_msg = from_json::<SwapMsg>(&msg.data)?;

            FORWARDING_STATE.remove(deps.storage);

            let new_balance = msg
                .to_token
                .get_balance(deps.as_ref(), env.contract.address.to_string())?;

            let swap_amount = new_balance.checked_sub(previous_balance)?;

            ensure!(
                swap_amount >= msg.minimum_receive,
                ContractError::MinReceived {
                    expected: msg.minimum_receive,
                    received: swap_amount,
                }
            );

            let forwading_msg = match swap_msg.forwarding_msg {
                Some(forwarding_msg) => Some(to_json_binary(&EuclidReceiverMsg::EuclidReceive(
                    forwarding_msg,
                ))?),
                None => None,
            };

            let transfer_msg = msg.to_token.create_transfer_msg(
                swap_amount,
                msg.recipient,
                None,
                forwading_msg,
            )?;

            Ok(Response::new()
                .add_attribute("action", "reply_astro_swap")
                .add_attribute("dex", "astroport")
                .add_attribute("complete_swap_in_amount", from_amount)
                .add_attribute("complete_swap_in_token", from_token.get_key())
                .add_attribute("complete_swap_out_amount", swap_amount)
                .add_attribute("complete_swap_out_token", msg.to_token.get_key())
                .add_message(transfer_msg))
        }
    }
}
