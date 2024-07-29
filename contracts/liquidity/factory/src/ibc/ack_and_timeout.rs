#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcAcknowledgement, IbcBasicResponse,
    IbcPacketAckMsg, IbcPacketTimeoutMsg, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128,
    WasmMsg,
};

use euclid::{
    error::ContractError,
    events::swap_event,
    liquidity::{AddLiquidityResponse, RemoveLiquidityResponse},
    msgs::{cw20::ExecuteMsg as Cw20ExecuteMsg, factory::ExecuteMsg},
    pool::PoolCreationResponse,
    swap::{SwapResponse, WithdrawResponse},
    token::Token,
};
use euclid_ibc::{ack::AcknowledgementMsg, msg::ChainIbcExecuteMsg};

use crate::{
    reply::{CW20_INSTANTIATE_REPLY_ID, ESCROW_INSTANTIATE_REPLY_ID, IBC_ACK_AND_TIMEOUT_REPLY_ID},
    state::{
        PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY,
        PENDING_SWAPS, STATE, TOKEN_TO_ESCROW, VLP_TO_CW20, VLP_TO_LP_SHARES,
    },
};

use super::channel::TIMEOUT_COUNTS;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    _deps: DepsMut,
    env: Env,
    ack: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let internal_msg = ExecuteMsg::IbcCallbackAckAndTimeout { ack: ack.clone() };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });
    let msg: Result<ChainIbcExecuteMsg, StdError> = from_json(&ack.original_packet.data);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    let sub_msg = SubMsg::reply_always(internal_msg, IBC_ACK_AND_TIMEOUT_REPLY_ID);
    Ok(IbcBasicResponse::new()
        .add_attribute("ibc_ack", ack.acknowledgement.data.to_string())
        .add_attribute("tx_id", tx_id)
        .add_submessage(sub_msg))
}

pub fn ibc_ack_packet_internal_call(
    deps: DepsMut,
    env: Env,
    ack: IbcPacketAckMsg,
) -> Result<Response, ContractError> {
    // Parse the ack based on request
    let msg: ChainIbcExecuteMsg = from_json(&ack.original_packet.data)?;
    match msg {
        ChainIbcExecuteMsg::RequestPoolCreation { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<PoolCreationResponse> =
                from_json(ack.acknowledgement.data)?;

            ack_pool_creation(deps, env, sender.address, res, tx_id)
        }

        ChainIbcExecuteMsg::AddLiquidity { tx_id, sender, .. } => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<AddLiquidityResponse> =
                from_json(ack.acknowledgement.data)?;
            ack_add_liquidity(deps, res, sender.address, tx_id)
        }
        ChainIbcExecuteMsg::RemoveLiquidity(msg) => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<RemoveLiquidityResponse> =
                from_json(ack.acknowledgement.data)?;
            ack_remove_liquidity(deps, res, msg.sender.address, msg.tx_id)
        }
        ChainIbcExecuteMsg::Swap(swap) => {
            // Process acknowledgment for swap
            let res: AcknowledgementMsg<SwapResponse> = from_json(ack.acknowledgement.data)?;
            ack_swap_request(deps, res, swap.sender.address, swap.tx_id)
        }
        ChainIbcExecuteMsg::Withdraw(msg) => {
            let res: AcknowledgementMsg<WithdrawResponse> = from_json(ack.acknowledgement.data)?;
            ack_withdraw_request(deps, res, msg.sender.address, msg.token, msg.tx_id)
        } // ChainIbcExecuteMsg::RequestWithdraw {
          //     token_id, tx_id, ..
          // } => {
          //     let res: AcknowledgementMsg<WithdrawResponse> = from_json(ack.acknowledgement.data)?;
          //     ack_request_withdraw(deps, res, token_id, tx_id)
          // }
          // ChainIbcExecuteMsg::RequestEscrowCreation { token, tx_id, .. } => {
          //     let res: AcknowledgementMsg<InstantiateEscrowResponse> =
          //         from_json(ack.acknowledgement.data)?;
          //     ack_request_instantiate_escrow(deps, env, res, token)
          // }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse, ContractError> {
    TIMEOUT_COUNTS.update(
        deps.storage,
        // timed out packets are sent by us, so lookup based on packet
        // source, not destination.
        msg.packet.src.channel_id.clone(),
        |count| -> StdResult<_> { Ok(count.unwrap_or_default() + 1) },
    )?;
    let failed_ack = IbcAcknowledgement::new(to_json_binary(&AcknowledgementMsg::Error::<()>(
        "Timeout".to_string(),
    ))?);

    let failed_ack_simulation = IbcPacketAckMsg::new(failed_ack, msg.packet, msg.relayer);

    // We want to handle timeout in same way we handle failed acknowledgement
    let result = ibc_packet_ack(deps, env, failed_ack_simulation);

    result.or(Ok(
        IbcBasicResponse::new().add_attribute("method", "ibc_packet_timeout")
    ))
}

// Function to create pool
fn ack_pool_creation(
    deps: DepsMut,
    env: Env,
    sender: String,
    res: AcknowledgementMsg<PoolCreationResponse>,
    tx_id: String,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender, tx_id.clone());
    let existing_req = PENDING_POOL_REQUESTS
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;

    // Remove pool request from MAP
    PENDING_POOL_REQUESTS.remove(deps.storage, req_key);

    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Load state to get escrow code id in case we need to instantiate
            let state = STATE.load(deps.storage)?;
            let escrow_code_id = state.escrow_code_id;
            let cw20_code_id = state.cw20_code_id;

            PAIR_TO_VLP.save(
                deps.storage,
                existing_req.pair_info.get_pair()?.get_tupple(),
                &data.vlp_contract.clone(),
            )?;
            // Prepare response
            let mut res = Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "pool_creation")
                .add_attribute("vlp", data.vlp_contract.clone());
            // Collects PairInfo into a vector of Token Info for easy iteration
            let tokens = existing_req.pair_info.get_vec_token_info();
            for token in tokens {
                let escrow_contract =
                    TOKEN_TO_ESCROW.may_load(deps.storage, token.token.clone())?;

                // Instantiate escrow if one doesn't exist
                if escrow_contract.is_none() {
                    let init_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                        admin: Some(env.contract.address.clone().into_string()),
                        code_id: escrow_code_id,
                        msg: to_json_binary(&euclid::msgs::escrow::InstantiateMsg {
                            token_id: token.token,
                            allowed_denom: Some(token.token_type),
                        })?,
                        funds: vec![],
                        label: "escrow".to_string(),
                    });

                    res = res.add_submessage(SubMsg {
                        id: ESCROW_INSTANTIATE_REPLY_ID,
                        msg: init_msg,
                        gas_limit: None,
                        reply_on: ReplyOn::Always,
                    });
                }
            }
            let lp_token_instantiate_data = existing_req.lp_token_instantiate_msg;
            // Instantiate cw20
            let init_cw20_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                admin: Some(state.admin.clone()),
                code_id: cw20_code_id,
                msg: to_json_binary(&euclid::msgs::cw20::InstantiateMsg {
                    name: lp_token_instantiate_data.name,
                    symbol: lp_token_instantiate_data.symbol,
                    decimals: lp_token_instantiate_data.decimals,
                    initial_balances: vec![],
                    mint: lp_token_instantiate_data.mint,
                    marketing: lp_token_instantiate_data.marketing,
                    vlp: data.vlp_contract,
                    factory: env.contract.address,
                    token_pair: existing_req.pair_info.get_pair()?,
                })?,
                funds: vec![],
                label: "cw20".to_string(),
            });

            Ok(res.add_submessage(SubMsg {
                id: CW20_INSTANTIATE_REPLY_ID,
                msg: init_cw20_msg,
                gas_limit: None,
                reply_on: ReplyOn::Always,
            }))
        }

        AcknowledgementMsg::Error(err) => Ok(Response::new()
            .add_attribute("tx_id", tx_id)
            .add_attribute("method", "reject_pool_request")
            .add_attribute("error", err.clone())),
    }
}

// Function to process add liquidity acknowledgment
fn ack_add_liquidity(
    deps: DepsMut,
    res: AcknowledgementMsg<AddLiquidityResponse>,
    sender: String,
    tx_id: String,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    // Validate that the pending exists for the sender
    let liquidity_info = PENDING_ADD_LIQUIDITY.load(deps.storage, req_key.clone())?;
    // Remove this from pending
    PENDING_ADD_LIQUIDITY.remove(deps.storage, req_key);
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove liquidity shares
            let shares = VLP_TO_LP_SHARES
                .may_load(deps.storage, data.vlp_address.clone())?
                .unwrap_or(Uint128::zero());
            let shares = shares.checked_add(data.mint_lp_tokens)?;

            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_address.clone(), &shares)?;
            // Prepare response
            let mut res = Response::new().add_attribute("method", "ack_add_liquidity");

            // Token 1
            let token_info = liquidity_info.pair_info.token_1.clone();
            let liquidity = liquidity_info.token_1_liquidity;
            let escrow_contract = TOKEN_TO_ESCROW.load(deps.storage, token_info.token.clone())?;

            let send_msg = token_info.create_escrow_msg(liquidity, escrow_contract)?;
            res = res.add_message(send_msg);

            // Token 2
            let token_info = liquidity_info.pair_info.token_2;
            let liquidity = liquidity_info.token_2_liquidity;
            let escrow_contract = TOKEN_TO_ESCROW.load(deps.storage, token_info.token.clone())?;
            let send_msg = token_info.create_escrow_msg(liquidity, escrow_contract)?;
            res = res.add_message(send_msg);

            // Mint cw20 tokens for sender //
            // Get cw20 contract address
            let cw20_address = VLP_TO_CW20.load(deps.storage, data.vlp_address)?;

            // Send mint msg
            let cw20_mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20_address.into_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Mint {
                    recipient: liquidity_info.sender,
                    amount: data.mint_lp_tokens,
                })?,
                funds: vec![],
            });

            Ok(res
                .add_message(cw20_mint_msg)
                .add_attribute("tx_id", tx_id)
                .add_attribute("sender", sender))
        }

        AcknowledgementMsg::Error(err) => {
            // Prepare messages to refund tokens back to user
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            let msg = liquidity_info.pair_info.token_1.create_transfer_msg(
                liquidity_info.token_1_liquidity,
                sender.to_string(),
                None,
            )?;
            msgs.push(msg);
            let msg = liquidity_info.pair_info.token_2.create_transfer_msg(
                liquidity_info.token_2_liquidity,
                sender.to_string(),
                None,
            )?;
            msgs.push(msg);

            Ok(Response::new()
                .add_attribute("method", "liquidity_tx_err_refund")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("error", err)
                .add_messages(msgs))
        }
    }
}

fn ack_remove_liquidity(
    deps: DepsMut,
    res: AcknowledgementMsg<RemoveLiquidityResponse>,
    sender: String,
    tx_id: String,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    // Validate that the pending exists for the sender
    let liquidity_info = PENDING_REMOVE_LIQUIDITY.load(deps.storage, req_key.clone())?;
    // Remove this from pending
    PENDING_REMOVE_LIQUIDITY.remove(deps.storage, req_key.clone());
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove liquidity shares
            let shares = VLP_TO_LP_SHARES
                .may_load(deps.storage, data.vlp_address.clone())?
                .unwrap_or(Uint128::zero());
            let shares = shares.checked_sub(data.burn_lp_tokens)?;

            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_address.clone(), &shares)?;
            // Prepare response
            let res = Response::new().add_attribute("method", "ack_remove_liquidity");

            // Burn cw20 tokens for sender //
            // Get cw20 contract address
            let cw20_address = VLP_TO_CW20.load(deps.storage, data.vlp_address)?;

            // Send burn msg
            let cw20_burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20_address.into_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Burn {
                    amount: liquidity_info.lp_allocation,
                })?,
                funds: vec![],
            });

            Ok(res
                .add_message(cw20_burn_msg)
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id))
        }

        // Todo:: Return LP Tokens back to sender
        AcknowledgementMsg::Error(err) => {
            // Send back cw20 to original sender
            let cw20_send_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: liquidity_info.cw20.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: sender.clone().into_string(),
                    amount: liquidity_info.lp_allocation,
                })?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_message(cw20_send_msg)
                .add_attribute("method", "liquidity_tx_err_refund")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("error", err))
        }
    }
}

// Function to process swap acknowledgment
// TODO this needs to be changed, callback msgs should probably sent to escrow
fn ack_swap_request(
    deps: DepsMut,
    res: AcknowledgementMsg<SwapResponse>,
    sender: String,
    tx_id: String,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    // Validate that the pending swap exists for the sender
    let swap_info = PENDING_SWAPS.load(deps.storage, (sender.clone(), tx_id.clone()))?;
    // Remove this from pending swaps
    PENDING_SWAPS.remove(deps.storage, (sender.clone(), tx_id.clone()));
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // TODO:: Add msg to send asset_in to escrow
            let asset_in = swap_info.asset_in.clone();

            // Get corresponding escrow
            let escrow_address = TOKEN_TO_ESCROW.load(deps.storage, asset_in.token.clone())?;

            let send_msg = asset_in.create_escrow_msg(swap_info.amount_in, escrow_address)?;
            let mut response = Response::new()
                .add_event(swap_event(&tx_id, &swap_info))
                .add_attribute("method", "process_successfull_swap")
                .add_message(send_msg)
                .add_attribute("tx_id", tx_id)
                .add_attribute("amount_out", data.amount_out)
                .add_attribute("swap_response", format!("{data:?}"));

            if !swap_info.partner_fee_amount.is_zero() {
                // Send the partner fee amount to recipient of the fee, if no recipient was provided, send the
                // funds back to the user
                let partner_fee_recipient = swap_info
                    .partner_fee_recipient
                    .unwrap_or(sender)
                    .to_string();
                let partner_send_msg = asset_in.create_transfer_msg(
                    swap_info.partner_fee_amount,
                    partner_fee_recipient.clone(),
                    None,
                )?;
                response = response
                    .add_message(partner_send_msg)
                    .add_attribute("partner_fee_amount", swap_info.partner_fee_amount)
                    .add_attribute("partner_fee_recipient", partner_fee_recipient)
            }

            Ok(response)
        }

        AcknowledgementMsg::Error(err) => {
            // Prepare messages to refund tokens back to user
            // Send back both amount in and fee amount
            let msg = swap_info.asset_in.create_transfer_msg(
                swap_info
                    .amount_in
                    .checked_add(swap_info.partner_fee_amount)?,
                sender.to_string(),
                None,
            )?;

            Ok(Response::new()
                .add_attribute("method", "process_failed_swap")
                .add_attribute("refund_to", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("refund_amount", swap_info.amount_in)
                .add_attribute("error", err)
                .add_message(msg))
        }
    }
}

fn ack_withdraw_request(
    _deps: DepsMut,
    res: AcknowledgementMsg<WithdrawResponse>,
    _sender: String,
    token_id: Token,
    _tx_id: String,
) -> Result<Response, ContractError> {
    match res {
        AcknowledgementMsg::Ok(_data) => {
            // Use it for logging, Router will send packets instead of ack to release tokens from escrow
            // Here you will get a response of escrows that router is going to release so it can be used in frontend

            Ok(Response::new()
                .add_attribute("method", "request_withdraw_submitted")
                .add_attribute("token", token_id.to_string()))
        }
        AcknowledgementMsg::Error(err) => Ok(Response::new()
            .add_attribute("method", "request_withdraw_error")
            .add_attribute("error", err.clone())),
    }
}

// fn ack_request_instantiate_escrow(
//     deps: DepsMut,
//     env: Env,
//     res: AcknowledgementMsg<InstantiateEscrowResponse>,
//     token_id: Token,
// ) -> Result<Response, ContractError> {
//     match res {
//         AcknowledgementMsg::Ok(data) => {
//             let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token_id.clone())?;
//             match escrow_address {
//                 Some(_) => Err(ContractError::EscrowAlreadyExists {}),
//                 None => {
//                     let msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
//                         admin: Some(env.contract.address.into_string()),
//                         code_id: data.escrow_code_id,
//                         msg: to_json_binary(&euclid::msgs::escrow::InstantiateMsg {
//                             token_id: token_id.clone(),
//                             allowed_denom: None,
//                         })?,
//                         funds: vec![],
//                         label: "escrow".to_string(),
//                     });
//                     Ok(Response::new()
//                         .add_submessage(SubMsg::reply_always(msg, ESCROW_INSTANTIATE_REPLY_ID))
//                         .add_attribute("method", "instantiate_escrow")
//                         .add_attribute("token", token_id.to_string()))
//                 }
//             }
//         }
//         AcknowledgementMsg::Error(err) => Ok(Response::new()
//             .add_attribute("method", "instantiate_escrow")
//             .add_attribute("error", err.clone())),
//     }
// }
