use cosmwasm_std::{ensure, DepsMut, Env, Reply, Response, SubMsgResult};
use euclid::error::ContractError;

use crate::state::FORWARDING_STATE;

pub const ASTRO_SWAP_REPLY_ID: u64 = 1;

pub fn on_astro_swap_reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::new(&format!(
            "Astroport swap failed: {}",
            err
        ))),
        SubMsgResult::Ok(..) => {
            let forwarding_state = FORWARDING_STATE.load(deps.storage)?;
            FORWARDING_STATE.remove(deps.storage);
            let new_balance = forwarding_state
                .to_token
                .get_balance(deps.as_ref(), env.contract.address.to_string())?;

            let swap_amount = new_balance.checked_sub(forwarding_state.previous_balance)?;

            ensure!(
                swap_amount >= forwarding_state.min_received,
                ContractError::MinReceived {
                    expected: forwarding_state.min_received,
                    received: swap_amount,
                }
            );

            let transfer_msg = forwarding_state.to_token.create_transfer_msg(
                swap_amount,
                forwarding_state.reciepient.to_string(),
                None,
                forwarding_state.forwarding_message,
            )?;

            Ok(Response::new()
                .add_attribute("action", "reply_astro_swap")
                .add_attribute("swap_amount", swap_amount.to_string())
                .add_message(transfer_msg))
        }
    }
}
