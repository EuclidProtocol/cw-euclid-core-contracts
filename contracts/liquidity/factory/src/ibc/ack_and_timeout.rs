#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcAcknowledgement, IbcBasicResponse,
    IbcPacketAckMsg, IbcPacketTimeoutMsg, ReplyOn, Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::MinterResponse;
use euclid::{
    error::ContractError,
    liquidity::{AddLiquidityResponse, RemoveLiquidityResponse},
    msgs::cw20::ExecuteMsg as Cw20ExecuteMsg,
    msgs::factory::ExecuteMsg,
    pool::PoolCreationResponse,
    swap::SwapResponse,
};
use euclid_ibc::{ack::AcknowledgementMsg, msg::ChainIbcExecuteMsg};

use crate::{
    reply::{CW20_INSTANTIATE_REPLY_ID, ESCROW_INSTANTIATE_REPLY_ID, IBC_ACK_AND_TIMEOUT_REPLY_ID},
    state::{
        PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY,
        PENDING_SWAPS, STATE, TOKEN_TO_CW20, TOKEN_TO_ESCROW, VLP_TO_LP_SHARES,
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
    let sub_msg = SubMsg::reply_always(internal_msg, IBC_ACK_AND_TIMEOUT_REPLY_ID);
    Ok(IbcBasicResponse::new()
        .add_attribute("ibc_ack", ack.acknowledgement.data.to_string())
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
            let escrow_code_id = STATE.load(deps.storage)?.escrow_code_id;
            let cw20_code_id = STATE.load(deps.storage)?.cw20_code_id;

            PAIR_TO_VLP.save(
                deps.storage,
                existing_req.pair_info.get_pair()?.get_tupple(),
                &data.vlp_contract.clone(),
            )?;
            // Prepare response
            let mut res = Response::new()
                .add_attribute("method", "pool_creation")
                .add_attribute("vlp", data.vlp_contract);
            // Collects PairInfo into a vector of Token Info for easy iteration
            let tokens = existing_req.pair_info.get_vec_token_info();
            for token in tokens {
                let escrow_contract =
                    TOKEN_TO_ESCROW.may_load(deps.storage, token.token.clone())?;
                match escrow_contract {
                    Some(escrow_address) => {
                        let token_allowed_query_msg =
                            euclid::msgs::escrow::QueryMsg::TokenAllowed {
                                denom: token.token_type.clone(),
                            };
                        let token_allowed: euclid::msgs::escrow::AllowedTokenResponse = deps
                            .querier
                            .query_wasm_smart(escrow_address.clone(), &token_allowed_query_msg)?;
                        if !token_allowed.allowed {
                            // Add allowed denom in existing escrow contract
                            let add_denom_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                                contract_addr: escrow_address.into_string(),
                                msg: to_json_binary(
                                    &euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
                                        denom: token.token_type,
                                    },
                                )?,
                                funds: vec![],
                            });
                            res = res.add_message(add_denom_msg);
                        }
                    }

                    None => {
                        // Instantiate escrow contract
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

                        // Needs to be submsg for reply event
                        res = res.add_submessage(SubMsg {
                            id: ESCROW_INSTANTIATE_REPLY_ID,
                            msg: init_msg,
                            gas_limit: None,
                            reply_on: ReplyOn::Always,
                        });
                    }
                }
            }
            let state = STATE.load(deps.storage)?;
            // Instantiate cw20
            let init_cw20_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                admin: Some(state.admin.clone()),
                code_id: cw20_code_id,
                msg: to_json_binary(&euclid::msgs::cw20::InstantiateMsg {
                    name: "cw20".to_string(),
                    symbol: format!(
                        "{}:{}",
                        existing_req.pair_info.token_1.token.to_string(),
                        existing_req.pair_info.token_2.token.to_string()
                    ),
                    decimals: 6,
                    initial_balances: vec![],
                    mint: Some(MinterResponse {
                        minter: state.admin,
                        cap: None,
                    }),
                    marketing: None,
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

            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_address, &shares)?;
            // Prepare response
            let mut res = Response::new().add_attribute("method", "ack_add_liquidity");

            // Token 1
            let token_info = liquidity_info.pair_info.token_1;
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
            let cw20_address = TOKEN_TO_CW20.load(deps.storage, token_info.token)?;

            // Send mint msg
            let cw20_mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cw20_address.into_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Mint {
                    recipient: liquidity_info.sender,
                    amount: liquidity_info.token_1_liquidity,
                })?,
                funds: vec![],
            });

            Ok(res
                .add_message(cw20_mint_msg)
                .add_attribute("sender", sender)
                .add_attribute("liquidity_id", tx_id))
        }

        AcknowledgementMsg::Error(err) => {
            // Prepare messages to refund tokens back to user
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            let msg = liquidity_info
                .pair_info
                .token_1
                .create_transfer_msg(liquidity_info.token_1_liquidity, sender.to_string())?;
            msgs.push(msg);
            let msg = liquidity_info
                .pair_info
                .token_1
                .create_transfer_msg(liquidity_info.token_2_liquidity, sender.to_string())?;
            msgs.push(msg);

            Ok(Response::new()
                .add_attribute("method", "liquidity_tx_err_refund")
                .add_attribute("sender", sender)
                .add_attribute("liquidity_id", tx_id)
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
    PENDING_REMOVE_LIQUIDITY.load(deps.storage, req_key.clone())?;
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

            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_address, &shares)?;
            // Prepare response
            let res = Response::new().add_attribute("method", "ack_remove_liquidity");
            Ok(res
                .add_attribute("sender", sender)
                .add_attribute("liquidity_id", tx_id))
        }

        // Todo:: Return LP Tokens back to sender
        AcknowledgementMsg::Error(err) => Ok(Response::new()
            .add_attribute("method", "liquidity_tx_err_refund")
            .add_attribute("sender", sender)
            .add_attribute("liquidity_id", tx_id)
            .add_attribute("error", err)),
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
    PENDING_SWAPS.remove(deps.storage, (sender.clone(), tx_id));
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // TODO:: Add msg to send asset_in to escrow
            let asset_in = swap_info.asset_in;

            // Get corresponding escrow
            let escrow_address = TOKEN_TO_ESCROW.load(deps.storage, asset_in.token.clone())?;

            let send_msg = asset_in.create_escrow_msg(swap_info.amount_in, escrow_address)?;

            Ok(Response::new()
                .add_message(send_msg)
                .add_attribute("method", "process_successfull_swap")
                .add_attribute("swap_response", format!("{data:?}")))
        }

        AcknowledgementMsg::Error(err) => {
            // Prepare messages to refund tokens back to user
            let msg = swap_info
                .asset_in
                .create_transfer_msg(swap_info.amount_in, sender.to_string())?;

            Ok(Response::new()
                .add_attribute("method", "process_failed_swap")
                .add_attribute("refund_to", "sender")
                .add_attribute("refund_amount", swap_info.amount_in)
                .add_attribute("error", err)
                .add_message(msg))
        }
    }
}

// fn ack_request_withdraw(
//     deps: DepsMut,
//     res: AcknowledgementMsg<WithdrawResponse>,
//     token_id: Token,
//     tx_id: String,
// ) -> Result<Response, ContractError> {
//     match res {
//         AcknowledgementMsg::Ok(_) => {
//             let _escrow_address = TOKEN_TO_ESCROW
//                 .load(deps.storage, token_id.clone())
//                 .map_err(|_err| ContractError::EscrowDoesNotExist {})?;

//             // Use it for logging, Router will send packets instead of ack to release tokens from escrow
//             // Here you will get a response of escrows that router is going to release so it can be used in frontend

//             Ok(Response::new()
//                 .add_attribute("method", "request_withdraw_submitted")
//                 .add_attribute("token", token_id.to_string()))
//         }
//         AcknowledgementMsg::Error(err) => Ok(Response::new()
//             .add_attribute("method", "request_withdraw_error")
//             .add_attribute("error", err.clone())),
//     }
// }

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
