#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_binary, to_binary, Binary, CosmosMsg, DepsMut, Env, IbcAcknowledgement, IbcBasicResponse,
    IbcPacketAckMsg, IbcPacketTimeoutMsg, ReplyOn, Response, StdError, SubMsg, Uint256, WasmMsg,
};
use euclid::{
    deposit::DepositTokenResponse,
    error::ContractError,
    events::{deposit_token_event, swap_event},
    liquidity::{AddLiquidityResponse, RemoveLiquidityResponse},
    msgs::{
        escrow::InstantiateMsg as EscrowInstantiateMsg, factory::ExecuteMsg,
        snip20::ExecuteMsg as Snip20ExecuteMsg,
    },
    pool::{DeRegisterDenomResponse, PoolCreationResponse, RegisterDenomResponse},
    swap::{SwapResponse, TransferResponse, WithdrawResponse},
    token::Token,
};
use euclid_ibc::{ack::AcknowledgementMsg, msg::ChainIbcExecuteMsg};
use secret_toolkit::utils::InitCallback;
use snip20_reference_impl::msg::InitConfig;

use crate::{
    query::get_contract_code_hash,
    reply::{
        ESCROW_INSTANTIATE_REPLY_ID, IBC_ACK_AND_TIMEOUT_REPLY_ID, SNIP20_INSTANTIATE_REPLY_ID,
    },
    state::{
        PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS,
        PENDING_DEPOSIT_TOKEN, PENDING_POOL_REQUESTS, PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS,
        PENDING_TOKEN_DEPOSIT, STATE, TOKEN_TO_ESCROW, VLP_TO_LP_SHARES, VLP_TO_SNIP20,
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
        code_hash: env.contract.code_hash,
        msg: to_binary(&internal_msg)?,
        funds: vec![],
    });
    let msg: Result<ChainIbcExecuteMsg, StdError> = from_binary(&ack.original_packet.data);
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
    let msg: ChainIbcExecuteMsg = from_binary(&ack.original_packet.data)?;
    reusable_internal_ack_call(deps, env, msg, ack.acknowledgement.data, false)
}
pub fn reusable_internal_ack_call(
    deps: DepsMut,
    env: Env,
    msg: ChainIbcExecuteMsg,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    // Parse the ack based on request
    match msg {
        ChainIbcExecuteMsg::RequestPoolCreation { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<PoolCreationResponse> = from_binary(&ack)?;

            ack_pool_creation(deps, env, sender.address, res, tx_id, is_native)
        }

        ChainIbcExecuteMsg::RegisterDenom { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<RegisterDenomResponse> = from_binary(&ack)?;

            ack_register_denom(deps, env, sender.address, res, tx_id, is_native)
        }
        ChainIbcExecuteMsg::DeRegisterDenom { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<DeRegisterDenomResponse> = from_binary(&ack)?;

            ack_deregister_denom(deps, env, sender.address, res, tx_id, is_native)
        }

        ChainIbcExecuteMsg::AddLiquidity { tx_id, sender, .. } => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<AddLiquidityResponse> = from_binary(&ack)?;
            ack_add_liquidity(deps, res, sender.address, tx_id, is_native)
        }
        ChainIbcExecuteMsg::RemoveLiquidity(msg) => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<RemoveLiquidityResponse> = from_binary(&ack)?;
            ack_remove_liquidity(deps, res, msg.sender.address, msg.tx_id, is_native)
        }
        ChainIbcExecuteMsg::Swap(swap) => {
            // Process acknowledgment for swap
            let res: AcknowledgementMsg<SwapResponse> = from_binary(&ack)?;
            ack_swap_request(deps, res, swap.sender.address, swap.tx_id, is_native)
        }
        ChainIbcExecuteMsg::Withdraw(msg) => {
            let res: AcknowledgementMsg<WithdrawResponse> = from_binary(&ack)?;
            ack_withdraw_request(
                deps,
                res,
                msg.sender.address,
                msg.token,
                msg.tx_id,
                is_native,
            )
        }
        ChainIbcExecuteMsg::Transfer(msg) => {
            let res: AcknowledgementMsg<TransferResponse> = from_binary(&ack)?;
            ack_transfer_request(
                deps,
                res,
                msg.sender.address,
                msg.token,
                msg.tx_id,
                is_native,
            )
        }
        ChainIbcExecuteMsg::DepositToken(deposit) => {
            // Process acknowledgment for deposit
            let res: AcknowledgementMsg<DepositTokenResponse> = from_binary(&ack)?;
            ack_deposit_token_request(deps, res, deposit.sender.address, deposit.tx_id, is_native)
        } // ChainIbcExecuteMsg::RequestWithdraw {
          //     token_id, tx_id, ..
          // } => {
          //     let res: AcknowledgementMsg<WithdrawResponse> = from_binary(ack.acknowledgement.data)?;
          //     ack_request_withdraw(deps, res, token_id, tx_id)
          // }
          // ChainIbcExecuteMsg::RequestEscrowCreation { token, tx_id, .. } => {
          //     let res: AcknowledgementMsg<InstantiateEscrowResponse> =
          //         from_binary(ack.acknowledgement.data)?;
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
    // timed out packets are sent by us, so lookup based on packet
    // source, not destination.
    let channel_id = msg.packet.src.channel_id.clone();

    // Fetch the current timeout count for the channel
    let current_count = TIMEOUT_COUNTS
        .get(deps.storage, &channel_id)
        .unwrap_or_default();

    let new_count = current_count + 1;

    // Insert the updated timeout count back into storage
    TIMEOUT_COUNTS.insert(deps.storage, &channel_id, &new_count)?;

    let failed_ack = IbcAcknowledgement::new(to_binary(&AcknowledgementMsg::Error::<()>(
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
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender, tx_id.clone());
    let existing_req = PENDING_POOL_REQUESTS
        .get(deps.storage, &req_key.clone())
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;

    // Remove pool request from MAP
    PENDING_POOL_REQUESTS.remove(deps.storage, &req_key)?;

    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Load state to get escrow code id in case we need to instantiate
            let state = STATE.load(deps.storage)?;
            let escrow_code_id = state.escrow_code_id;
            let escrow_code_hash = state.escrow_code_hash;
            let snip20_code_id = state.snip20_code_id;
            let snip20_code_hash = state.snip20_code_hash;

            PAIR_TO_VLP.insert(
                deps.storage,
                &existing_req.pair_info.get_pair()?.get_tupple(),
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
                if token.token_type.is_voucher() {
                    continue;
                }

                // Instantiate escrow if one doesn't exist
                // if escrow_contract.is_none() {
                //     let init_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                //         admin: Some(state.admin.clone()),
                //         code_id: escrow_code_id,
                //         msg: to_binary(&EscrowInstantiateMsg {
                //             token_id: token.token,
                //             allowed_denom: Some(token.token_type),
                //         })?,
                //         funds: vec![],
                //         label: "escrow".to_string(),
                //     });

                //     res = res.add_submessage(SubMsg {
                //         id: ESCROW_INSTANTIATE_REPLY_ID,
                //         msg: init_msg,
                //         gas_limit: None,
                //         reply_on: ReplyOn::Always,
                //     });
                // }
                match TOKEN_TO_ESCROW.get(deps.storage, &token.token.clone()) {
                    Some(escrow_contract) => {
                        let send_msg = token.token_type.create_escrow_msg(
                            token.amount,
                            escrow_contract.addr,
                            escrow_contract.code_hash,
                            None,
                            None,
                            None,
                            None,
                        )?;
                        res = res.add_message(send_msg);
                    }
                    // Instantiate escrow if one doesn't exist
                    None => {
                        // let init_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                        //     admin: Some(state.admin.clone()),
                        //     code_id: escrow_code_id,
                        //     code_hash: escrow_code_hash.clone(),
                        //     msg: to_binary(&EscrowInstantiateMsg {
                        //         token_id: token.clone().token,
                        //         allowed_denom: Some(token.clone().token_type),
                        //     })?,
                        //     funds: vec![],
                        //     label: "escrow".to_string(),
                        // });

                        let init_msg = EscrowInstantiateMsg {
                            token_id: token.clone().token,
                            allowed_denom: Some(token.clone().token_type),
                        };
                        PENDING_DEPOSIT_TOKEN.insert(deps.storage, &token.clone().token, &token)?;
                        res = res.add_submessage(SubMsg {
                            id: ESCROW_INSTANTIATE_REPLY_ID,
                            msg: init_msg.to_cosmos_msg(
                                Some(state.admin.clone()),
                                "escrow".to_string(),
                                escrow_code_id,
                                escrow_code_hash.clone(),
                                None,
                            )?,
                            gas_limit: None,
                            reply_on: ReplyOn::Always,
                        });
                    }
                }
            }
            let lp_token_instantiate_data = existing_req.lp_token_instantiate_msg;

            // let init_snip20_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
            //     admin: Some(state.admin.clone()),
            //     code_id: snip20_code_id,
            //     code_hash: snip20_code_hash,
            //     msg: to_binary(&euclid::msgs::snip20::InstantiateMsg {
            //         name: lp_token_instantiate_data.name,
            //         symbol: lp_token_instantiate_data.symbol,
            //         decimals: lp_token_instantiate_data.decimals,
            //         initial_balances: vec![],
            //         vlp: data.vlp_contract,
            //         factory: env.contract.address,
            //         token_pair: existing_req.pair_info.get_pair()?,
            //         admin: None,
            //         prng_seed: to_binary(&"seed")?,
            //         config: None,
            //         supported_denoms: None,
            //     })?,
            //     funds: vec![],
            //     label: "snip20".to_string(),
            // });

            // Instantiate snip20
            let init_snip20_msg = euclid::msgs::snip20::InstantiateMsg {
                name: lp_token_instantiate_data.name,
                symbol: lp_token_instantiate_data.symbol,
                decimals: lp_token_instantiate_data.decimals,
                initial_balances: vec![],
                vlp: data.vlp_contract,
                factory: env.contract.address,
                token_pair: existing_req.pair_info.get_pair()?,
                admin: Some(state.admin.clone()),
                prng_seed: to_binary(&"seed")?,
                config: Some(InitConfig {
                    public_total_supply: Some(true),
                    enable_deposit: Some(true),
                    enable_redeem: Some(true),
                    enable_mint: Some(true),
                    enable_burn: Some(true),
                    can_modify_denoms: Some(true),
                }),
                supported_denoms: None,
            };

            Ok(res.add_submessage(SubMsg {
                id: SNIP20_INSTANTIATE_REPLY_ID,
                msg: init_snip20_msg.to_cosmos_msg(
                    Some(state.admin.clone()),
                    "snip20".to_string(),
                    snip20_code_id,
                    snip20_code_hash,
                    None,
                )?,
                gas_limit: None,
                reply_on: ReplyOn::Always,
            }))
        }

        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "reject_pool_request")
                .add_attribute("error", err.clone()))
        }
    }
}

fn ack_register_denom(
    deps: DepsMut,
    _env: Env,
    sender: String,
    res: AcknowledgementMsg<RegisterDenomResponse>,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender, tx_id.clone());
    let existing_req = PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
        .get(deps.storage, &req_key.clone())
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;

    // Remove pool request from MAP
    PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS.remove(deps.storage, &req_key)?;

    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(_data) => {
            let state = STATE.load(deps.storage)?;
            let escrow_code_id = state.escrow_code_id;
            let escrow_code_hash = state.escrow_code_hash;
            let token = existing_req.token;

            // let existing_escrow = TOKEN_TO_ESCROW.get(deps.storage, &token.token.clone())?;

            let mut response = Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "ack_register_denom")
                .add_attribute("token", token.token.to_string())
                .add_attribute("token_type", token.token_type.get_key());

            if let Some(escrow) = TOKEN_TO_ESCROW.get(deps.storage, &token.token.clone()) {
                let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: escrow.addr.into_string(),
                    code_hash: escrow.code_hash,
                    msg: to_binary(&euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
                        denom: token.token_type.clone(),
                    })?,
                    funds: vec![],
                });
                response = response
                    .add_attribute("create_escrow", "false")
                    .add_message(msg);
            } else {
                // Instantiate escrow
                let init_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                    admin: Some(state.admin.clone()),
                    code_id: escrow_code_id,
                    code_hash: escrow_code_hash,
                    msg: to_binary(&EscrowInstantiateMsg {
                        token_id: token.token,
                        allowed_denom: Some(token.token_type),
                    })?,
                    funds: vec![],
                    label: "escrow".to_string(),
                });
                response = response
                    .add_attribute("create_escrow", "true")
                    .add_submessage(SubMsg {
                        id: ESCROW_INSTANTIATE_REPLY_ID,
                        msg: init_msg,
                        gas_limit: None,
                        reply_on: ReplyOn::Always,
                    });
            }

            Ok(response)
        }

        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "reject_denom_register")
                .add_attribute("error", err.clone()))
        }
    }
}

fn ack_deregister_denom(
    deps: DepsMut,
    _env: Env,
    sender: String,
    res: AcknowledgementMsg<DeRegisterDenomResponse>,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender, tx_id.clone());
    let existing_req = PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
        .get(deps.storage, &req_key.clone())
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;

    // Remove pool request from MAP
    PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS.remove(deps.storage, &req_key)?;

    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(_data) => {
            let token = existing_req.token;

            let escrow = TOKEN_TO_ESCROW
                .get(deps.storage, &token.token.clone())
                .unwrap();

            let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: escrow.addr.into_string(),
                code_hash: escrow.code_hash,
                msg: to_binary(&euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
                    denom: token.token_type.clone(),
                })?,
                funds: vec![],
            });

            Ok(Response::new()
                .add_message(msg)
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "ack_deregister_denom")
                .add_attribute("token", token.token.to_string())
                .add_attribute("token_type", token.token_type.get_key()))
        }

        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "reject_denom_deregister")
                .add_attribute("error", err.clone()))
        }
    }
}

// Function to process add liquidity acknowledgment
fn ack_add_liquidity(
    deps: DepsMut,
    res: AcknowledgementMsg<AddLiquidityResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    // Validate that the pending exists for the sender
    let liquidity_info = PENDING_ADD_LIQUIDITY
        .get(deps.storage, &req_key.clone())
        .unwrap();
    // Remove this from pending
    PENDING_ADD_LIQUIDITY.remove(deps.storage, &req_key)?;
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove liquidity shares
            let shares = VLP_TO_LP_SHARES
                .get(deps.storage, &data.vlp_address.clone())
                .unwrap_or(Uint256::zero());
            let shares = shares.checked_add(data.mint_lp_tokens.into())?;

            VLP_TO_LP_SHARES.insert(deps.storage, &data.vlp_address.clone(), &shares)?;
            // Prepare response
            let mut res = Response::new().add_attribute("method", "ack_add_liquidity");

            // Send tokens back to escrow
            for token_info in liquidity_info.pair_info.get_vec_token_info() {
                // Vouchers are not escrowed
                if token_info.token_type.is_voucher() {
                    continue;
                }

                let liquidity = token_info.amount;
                let escrow = TOKEN_TO_ESCROW
                    .get(deps.storage, &token_info.token.clone())
                    .unwrap();
                let send_msg = token_info.token_type.create_escrow_msg(
                    liquidity,
                    escrow.addr,
                    escrow.code_hash,
                    None,
                    None,
                    None,
                    None,
                )?;
                res = res.add_message(send_msg);
            }

            // Mint snip20 tokens for sender //
            // Get snip20 contract address
            let snip20_address = VLP_TO_SNIP20.get(deps.storage, &data.vlp_address).unwrap();
            let snip20_code_hash =
                get_contract_code_hash(deps.querier, snip20_address.clone().into_string())?;

            // Send mint msg
            let snip20_mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: snip20_address.into_string(),
                code_hash: snip20_code_hash,
                msg: to_binary(&Snip20ExecuteMsg::Mint {
                    recipient: liquidity_info.sender,
                    amount: data.mint_lp_tokens,
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })?,
                funds: vec![],
            });

            Ok(res
                .add_message(snip20_mint_msg)
                .add_attribute("tx_id", tx_id)
                .add_attribute("sender", sender))
        }

        AcknowledgementMsg::Error(err) => {
            // Its a native call so you can return error to reject complete execution call
            if is_native {
                return Err(ContractError::new(&err));
            }
            // Prepare messages to refund tokens back to user
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            for token_info in liquidity_info.pair_info.get_vec_token_info() {
                if token_info.token_type.is_voucher() {
                    continue;
                }
                let msg = token_info.token_type.create_transfer_msg(
                    token_info.amount,
                    sender.to_string(),
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;
                msgs.push(msg);
            }

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
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    // Validate that the pending exists for the sender
    let liquidity_info = PENDING_REMOVE_LIQUIDITY
        .get(deps.storage, &req_key.clone())
        .unwrap();
    // Remove this from pending
    PENDING_REMOVE_LIQUIDITY.remove(deps.storage, &req_key.clone())?;
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove liquidity shares
            let shares = VLP_TO_LP_SHARES
                .get(deps.storage, &data.vlp_address.clone())
                .unwrap_or(Uint256::zero());
            let shares = shares.checked_sub(data.burn_lp_tokens.into())?;

            VLP_TO_LP_SHARES.insert(deps.storage, &data.vlp_address.clone(), &shares)?;
            // Prepare response
            let res = Response::new().add_attribute("method", "ack_remove_liquidity");

            // Burn snip20 tokens for sender //
            // Get snip20 contract address
            let snip20_address = VLP_TO_SNIP20.get(deps.storage, &data.vlp_address).unwrap();
            let snip20_code_hash =
                get_contract_code_hash(deps.querier, snip20_address.clone().into_string())?;

            // Send burn msg
            let snip20_burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: snip20_address.into_string(),
                code_hash: snip20_code_hash,
                msg: to_binary(&Snip20ExecuteMsg::Burn {
                    amount: liquidity_info.lp_allocation,
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })?,
                funds: vec![],
            });

            Ok(res
                .add_message(snip20_burn_msg)
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id))
        }

        AcknowledgementMsg::Error(err) => {
            // Its a native call so you can return error to reject complete execution call
            if is_native {
                return Err(ContractError::new(&err));
            }
            // Send back snip20 to original sender
            let snip20_send_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: liquidity_info.snip20.to_string(),
                code_hash: get_contract_code_hash(deps.querier, liquidity_info.snip20.to_string())?,
                msg: to_binary(&Snip20ExecuteMsg::Transfer {
                    recipient: sender.clone().into_string(),
                    amount: liquidity_info.lp_allocation,
                    memo: None,
                    decoys: None,
                    entropy: None,
                    padding: None,
                })?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_message(snip20_send_msg)
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
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    // Validate that the pending swap exists for the sender
    let swap_info = PENDING_SWAPS
        .get(deps.storage, &(sender.clone(), tx_id.clone()))
        .unwrap();
    // Remove this from pending swaps
    PENDING_SWAPS.remove(deps.storage, &(sender.clone(), tx_id.clone()))?;
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            let asset_in = swap_info.asset_in.clone();

            let mut response = Response::new()
                .add_event(swap_event(&tx_id, &swap_info))
                .add_attribute("method", "process_successfull_swap")
                .add_attribute("tx_id", tx_id)
                .add_attribute("amount_out", data.amount_out)
                .add_attribute("swap_response", format!("{data:?}"))
                .add_attribute("partner_fee_amount", swap_info.partner_fee_amount)
                .add_attribute("partner_fee_recipient", &swap_info.partner_fee_recipient);

            if !swap_info.partner_fee_amount.is_zero() {
                let mut state = STATE.load(deps.storage)?;

                // Add partner fee collected to the total
                state
                    .partner_fees_collected
                    .add_fee(asset_in.token.to_string(), swap_info.partner_fee_amount);

                // Save new total partner fees collected to state
                STATE.save(deps.storage, &state)?;
            }
            if !asset_in.token_type.is_voucher() {
                let escrow = TOKEN_TO_ESCROW
                    .get(deps.storage, &asset_in.token.clone())
                    .unwrap();
                let send_msg = asset_in.create_escrow_msg(
                    swap_info.amount_in,
                    escrow.addr,
                    escrow.code_hash,
                    None,
                    None,
                    None,
                    None,
                )?;
                response = response.add_message(send_msg);

                // if partner fee is not zero, send it to the partner fee recipient
                if !swap_info.partner_fee_amount.is_zero() {
                    let partner_send_msg = asset_in.create_transfer_msg(
                        swap_info.partner_fee_amount,
                        swap_info.partner_fee_recipient.to_string(),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )?;
                    response = response.add_message(partner_send_msg)
                }
            }

            Ok(response)
        }

        AcknowledgementMsg::Error(err) => {
            // Its a native call so you can return error to reject complete execution call
            if is_native {
                return Err(ContractError::new(&err));
            }
            let mut response = Response::new()
                .add_attribute("method", "process_failed_swap")
                .add_attribute("refund_to", &sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("refund_amount", swap_info.amount_in)
                .add_attribute("error", err);
            // Prepare messages to refund tokens back to user
            // Send back both amount in and fee amount
            // NOTE - Only return the amount in if the token is not a voucher
            if !swap_info.asset_in.token_type.is_voucher() {
                let msg = swap_info.asset_in.create_transfer_msg(
                    swap_info
                        .amount_in
                        .checked_add(swap_info.partner_fee_amount)?,
                    sender.to_string(),
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;
                response = response.add_message(msg);
            }

            Ok(response)
        }
    }
}

fn ack_deposit_token_request(
    deps: DepsMut,
    res: AcknowledgementMsg<DepositTokenResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    // Validate that the pending swap exists for the sender
    let deposit_info = PENDING_TOKEN_DEPOSIT
        .get(deps.storage, &(sender.clone(), tx_id.clone()))
        .unwrap();
    // Remove this from pending swaps
    PENDING_TOKEN_DEPOSIT.remove(deps.storage, &(sender.clone(), tx_id.clone()))?;
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            let asset_in = deposit_info.asset_in.clone();

            // Get corresponding escrow
            let escrow = TOKEN_TO_ESCROW
                .get(deps.storage, &asset_in.token.clone())
                .unwrap();

            let send_msg = asset_in.create_escrow_msg(
                data.amount,
                escrow.addr,
                escrow.code_hash,
                None,
                None,
                None,
                None,
            )?;
            let response = Response::new()
                .add_event(deposit_token_event(&tx_id, &deposit_info))
                .add_attribute("method", "process_successfull_deposit_token")
                .add_message(send_msg)
                .add_attribute("tx_id", tx_id)
                .add_attribute("deposit_token_response", format!("{data:?}"));

            Ok(response)
        }

        AcknowledgementMsg::Error(err) => {
            // Its a native call so you can return error to reject complete execution call
            if is_native {
                return Err(ContractError::new(&err));
            }
            // Prepare messages to refund tokens back to user
            // Send back both amount in and fee amount
            let msg = deposit_info.asset_in.create_transfer_msg(
                deposit_info.amount_in,
                sender.to_string(),
                None,
                None,
                None,
                None,
                None,
            )?;

            Ok(Response::new()
                .add_attribute("method", "process_failed_deposit_token")
                .add_attribute("refund_to", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("refund_amount", deposit_info.amount_in)
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
    is_native: bool,
) -> Result<Response, ContractError> {
    match res {
        AcknowledgementMsg::Ok(_data) => {
            // Use it for logging, Router will send packets instead of ack to release tokens from escrow
            // Here you will get a response of escrows that router is going to release so it can be used in frontend

            Ok(Response::new()
                .add_attribute("method", "request_withdraw_submitted")
                .add_attribute("token", token_id.to_string()))
        }
        AcknowledgementMsg::Error(err) => {
            // Its a native call so you can return error to reject complete execution call
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "request_withdraw_error")
                .add_attribute("error", err.clone()))
        }
    }
}

fn ack_transfer_request(
    _deps: DepsMut,
    res: AcknowledgementMsg<TransferResponse>,
    _sender: String,
    token_id: Token,
    _tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    match res {
        AcknowledgementMsg::Ok(_data) => {
            // Use it for logging, Router will send packets instead of ack to release tokens from escrow
            // Here you will get a response of escrows that router is going to release so it can be used in frontend

            Ok(Response::new()
                .add_attribute("method", "transfer")
                .add_attribute("token", token_id.to_string()))
        }
        AcknowledgementMsg::Error(err) => {
            // Its a native call so you can return error to reject complete execution call
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "transfer_error")
                .add_attribute("error", err.clone()))
        }
    }
}
