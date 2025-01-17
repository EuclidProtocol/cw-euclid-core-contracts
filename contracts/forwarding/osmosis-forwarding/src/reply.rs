use crate::state::{ForwardingState, FORWARDING_STATE};
use cosmwasm_std::{
    ensure, to_json_binary, Coin, Decimal, DepsMut, Env, Reply, Response, SubMsgResult,
};
use euclid::{error::ContractError, msgs::hook::EuclidReceiverMsg, token::TokenType};
use swaprouter::msg::Slippage as OsmosisSlippage;

pub const OSMO_SWAP_REPLY_ID: u64 = 1;

pub fn on_osmo_swap_reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::new(&format!("Osmosis swap failed: {}", err))),
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
            match swap_msg.slippage {
                OsmosisSlippage::MinOutputAmount(min_output_amount) => {
                    ensure!(
                        swap_amount >= min_output_amount,
                        ContractError::MinReceived {
                            expected: min_output_amount,
                            received: swap_amount,
                        }
                    );
                }
                OsmosisSlippage::Twap {
                    window_seconds: _,
                    slippage_percentage,
                } => {
                    let input_coin = match from_token {
                        TokenType::Native { ref denom } => Coin {
                            denom: denom.to_string(),
                            amount: from_amount,
                        },
                        TokenType::Smart {
                            ref contract_address,
                        } => Coin {
                            denom: contract_address.to_string(),
                            amount: from_amount,
                        },
                        _ => return Err(ContractError::new("unsupported token type")),
                    };

                    let min_output_amount =
                        input_coin.amount * (Decimal::one() - slippage_percentage);
                    ensure!(
                        swap_amount >= min_output_amount,
                        ContractError::MinReceived {
                            expected: min_output_amount,
                            received: swap_amount,
                        }
                    );
                }
            }
            // ensure!(
            //     swap_amount >= swap_msg.input_coin.amount,
            //     ContractError::MinReceived {
            //         expected: swap_msg.minimum_receive,
            //         received: swap_amount,
            //     }
            // );

            let forwading_msg = match swap_msg.forwarding_msg {
                Some(forwarding_msg) => Some(to_json_binary(&EuclidReceiverMsg::EuclidReceive(
                    forwarding_msg,
                ))?),
                None => None,
            };

            let transfer_msg = swap_msg.to_token.create_transfer_msg(
                swap_amount,
                swap_msg.recipient,
                None,
                forwading_msg,
            )?;

            Ok(Response::new()
                .add_attribute("action", "reply_osmo_swap")
                .add_attribute("dex", "osmosis")
                .add_attribute("complete_swap_in_amount", from_amount)
                .add_attribute("complete_swap_in_token", from_token.get_key())
                .add_attribute("complete_swap_out_amount", swap_amount)
                .add_attribute("complete_swap_out_token", swap_msg.to_token.get_key())
                .add_message(transfer_msg))
        }
    }
}
