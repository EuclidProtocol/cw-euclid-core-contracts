use crate::{
    math::compute_swap,
    state::{self, AMP_FACTOR, BALANCES, DEFAULT_AMP_FACTOR, STATE},
};
use cosmwasm_std::{
    ensure, to_json_binary, Decimal, Decimal256, DepsMut, Env, Response, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    events::{liquidity_event, tx_event, TxType},
    msgs::{stable_vlp::VlpSwapResponse, virtual_balance::ExecuteTransfer},
    pool::{NEXT_SWAP_REPLY_ID, VIRTUAL_BALANCE_TRANSFER_REPLY_ID},
    swap::NextSwapVlp,
    token::Token,
    utils::math::Decimal256Ext,
};

#[allow(clippy::too_many_arguments)]
pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    sender: CrossChainUser,
    asset_in: Token,
    amount_in: Uint128,
    min_token_out: Uint128,
    tx_id: String,
    next_swaps: Vec<NextSwapVlp>,
    test_fail: Option<bool>,
) -> Result<Response, ContractError> {
    ensure!(
        !test_fail.unwrap_or(false),
        ContractError::new("Force fail flag")
    );
    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let mut state = state::STATE.load(deps.storage)?;

    let pair = state.pair.clone();

    ensure!(asset_in.exists(pair), ContractError::AssetDoesNotExist {});
    let asset_out = state.pair.get_other_token(asset_in.clone());

    let mut token_in_reserve = BALANCES.load(deps.storage, asset_in.clone())?;
    let mut token_out_reserve = BALANCES.load(deps.storage, asset_out.clone())?;

    // Swap needs approval to use voucher tokens
    let transfer_voucher_msg =
        euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
            amount: amount_in,
            token_id: asset_in.to_string(),
            from: sender.clone(),
            to: CrossChainUser {
                address: env.contract.address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
        });

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: state.virtual_balance.clone(),
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };

    let mut response = Response::new();
    // Should reject full execution if failed
    response = response.add_message(transfer_voucher_msg);

    // Get Fee from the state
    let fee = state.clone().fee;

    let lp_fee = amount_in.checked_mul_floor(Decimal::bps(fee.lp_fee_bps))?;
    let euclid_fee = amount_in.checked_mul_floor(Decimal::bps(fee.euclid_fee_bps))?;

    // Add the lp fee to total fees
    state
        .total_fees_collected
        .lp_fees
        .add_fee(asset_in.to_string(), lp_fee);

    // Calcuate the sum of fees
    let total_fee = lp_fee.checked_add(euclid_fee)?;

    // Calculate the amount of asset to be swapped
    let swap_amount = amount_in.checked_sub(total_fee)?;

    let amp_factor = AMP_FACTOR.load(deps.storage).unwrap_or(DEFAULT_AMP_FACTOR);

    let receive_amount = compute_swap(
        &Decimal256::from_integer(amount_in),
        &Decimal256::from_integer(token_in_reserve),
        &Decimal256::from_integer(token_out_reserve),
        amp_factor,
    )?
    .return_amount;
    println!("receive_amount: {}", receive_amount);

    // Verify that the receive amount is greater than 0 to be eligible for any swap
    ensure!(
        !receive_amount.is_zero(),
        ContractError::SlippageExceeded {
            amount: receive_amount,
            min_amount_out: min_token_out,
        }
    );

    token_in_reserve = token_in_reserve
        .checked_add(swap_amount)?
        .checked_add(lp_fee)?;
    token_out_reserve = token_out_reserve.checked_sub(receive_amount)?;

    ensure!(
        !token_out_reserve.is_zero(),
        ContractError::new("Token out reserve is zero")
    );

    BALANCES.save(deps.storage, asset_in.clone(), &token_in_reserve)?;
    BALANCES.save(deps.storage, asset_out.clone(), &token_out_reserve)?;

    // Finalize ack response to swap pool
    let swap_response = VlpSwapResponse {
        sender: sender.clone(),
        tx_id: tx_id.clone(),
        asset_out: asset_out.clone(),
        amount_out: receive_amount,
    };

    // Prepare acknowledgement
    let acknowledgement = to_json_binary(&swap_response)?;

    if !euclid_fee.is_zero() {
        // Add the euclid fee to total fees
        state
            .total_fees_collected
            .euclid_fees
            .add_fee(asset_in.to_string(), euclid_fee);

        let euclid_fee_transfer_msg =
            euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                amount: euclid_fee,
                token_id: asset_in.to_string(),

                // Source Address
                from: CrossChainUser {
                    address: env.contract.address.to_string(),
                    chain_uid: ChainUid::vsl_chain_uid()?,
                },

                // Destination Address
                to: fee.recipient,
            });

        let euclid_fee_transfer_msg = WasmMsg::Execute {
            contract_addr: state.virtual_balance.clone(),
            msg: to_json_binary(&euclid_fee_transfer_msg)?,
            funds: vec![],
        };

        let euclid_fee_transfer_msg =
            SubMsg::reply_on_error(euclid_fee_transfer_msg, VIRTUAL_BALANCE_TRANSFER_REPLY_ID);

        response = response.add_submessage(euclid_fee_transfer_msg);
    }

    match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            // There are more swaps
            let virtual_balance_transfer_msg =
                euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                    amount: swap_response.amount_out,
                    token_id: swap_response.asset_out.to_string(),

                    from: CrossChainUser {
                        address: env.contract.address.to_string(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },

                    to: CrossChainUser {
                        address: next_swap.vlp_address.clone(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },
                });

            let virtual_balance_transfer_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance.clone(),
                msg: to_json_binary(&virtual_balance_transfer_msg)?,
                funds: vec![],
            };

            let virtual_balance_transfer_msg = SubMsg::reply_on_error(
                virtual_balance_transfer_msg,
                VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
            );

            let next_swap_msg = euclid::msgs::vlp::ExecuteMsg::Swap {
                sender: sender.clone(),
                // Final user address and chain id

                // Carry forward amount to next swap
                asset_in: swap_response.asset_out,
                amount_in: swap_response.amount_out,
                min_token_out,
                tx_id: tx_id.clone(),
                next_swaps: forward_swaps.to_vec(),
                test_fail: next_swap.test_fail,
            };
            let next_swap_msg = WasmMsg::Execute {
                contract_addr: next_swap.vlp_address.clone(),
                msg: to_json_binary(&next_swap_msg)?,
                funds: vec![],
            };

            let next_swap_msg = SubMsg::reply_always(next_swap_msg, NEXT_SWAP_REPLY_ID);

            response = response
                .add_attribute("swap_type", "forward_swap")
                .add_attribute("forward_to", next_swap.vlp_address.clone())
                .add_submessage(virtual_balance_transfer_msg)
                .add_submessage(next_swap_msg);
        }
        None => {
            //Its the last swap

            // Verify that the receive amount is >= min amount as its last swap
            ensure!(
                receive_amount.ge(&min_token_out),
                ContractError::SlippageExceeded {
                    amount: receive_amount,
                    min_amount_out: min_token_out,
                }
            );

            let virtual_balance_transfer_msg =
                euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                    amount: swap_response.amount_out,
                    token_id: swap_response.asset_out.to_string(),

                    // Source Address
                    from: CrossChainUser {
                        address: env.contract.address.to_string(),
                        chain_uid: ChainUid::vsl_chain_uid()?,
                    },

                    // Destination Address
                    to: sender.clone(),
                });

            let virtual_balance_transfer_msg = WasmMsg::Execute {
                contract_addr: state.virtual_balance.clone(),
                msg: to_json_binary(&virtual_balance_transfer_msg)?,
                funds: vec![],
            };

            let virtual_balance_transfer_msg = SubMsg::reply_on_error(
                virtual_balance_transfer_msg,
                VIRTUAL_BALANCE_TRANSFER_REPLY_ID,
            );

            response = response
                .add_attribute("swap_type", "final_swap")
                .add_attribute("receiver_address", sender.address.clone())
                .add_attribute("receiver_chain_id", sender.chain_uid.to_string())
                .add_submessage(virtual_balance_transfer_msg);
        }
    };

    // Save changes to total fees in state
    STATE.save(deps.storage, &state)?;

    Ok(response
        .add_event(tx_event(&tx_id, &sender.to_sender_string(), TxType::Swap))
        .add_event(liquidity_event(
            &[
                asset_in.with_amount(token_in_reserve),
                asset_out.with_amount(token_out_reserve),
            ],
            &[
                asset_in.with_amount(swap_amount.checked_add(lp_fee)?),
                asset_out.with_amount(receive_amount),
            ],
            &tx_id,
        ))
        .add_attribute("action", "swap")
        .add_attribute("amount_in", amount_in)
        .add_attribute("asset_in", asset_in.to_string())
        .add_attribute("asset_out", asset_out.to_string())
        .add_attribute("total_fee", total_fee)
        .add_attribute("euclid_fee", euclid_fee)
        .add_attribute("lp_fee", lp_fee)
        .add_attribute("receive_amount", receive_amount)
        .set_data(acknowledgement))
}
