#[cfg(not(feature = "library"))]
use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, Int256, ReplyOn, Response,
    SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20Coin;
use euclid::{
    deposit::DepositTokenResponse,
    error::ContractError,
    events::{deposit_token_event, swap_event},
    liquidity::{
        AddLiquidityResponse, ConcentratedAddLiquidityResponse, ConcentratedCollectFeesResponse,
        ConcentratedCollectProtocolFeesResponse, ConcentratedRemoveLiquidityResponse,
        RemoveLiquidityResponse,
    },
    msgs::{
        self,
        escrow::InstantiateMsg as EscrowInstantiateMsg,
        position_token::{self, PositionInfoResponse},
        vlp::base::{DeregisterDenomResponse, RegisterDenomResponse},
    },
    swap::{SwapResponse, TransferVoucherResponse},
    token::Token,
};
use euclid_ibc::{
    ack::AcknowledgementMsg,
    router_ibc::{
        RouterCrossChainConcentratedRequestPoolCreationExecuteMsg, RouterCrossChainExecuteMsg,
    },
};

use crate::{
    reply::{ESCROW_INSTANTIATE_REPLY_ID, LP_INSTANTIATE_REPLY_ID},
    state::{
        ADMIN, FEE_STATE, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_CONCENTRATED_ADD_LIQUIDITY,
        PENDING_CONCENTRATED_COLLECT_FEES, PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES,
        PENDING_CONCENTRATED_POOL_REQUESTS, PENDING_CONCENTRATED_REMOVE_LIQUIDITY,
        PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS, PENDING_DEPOSIT_TOKEN, PENDING_POOL_REQUESTS,
        PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, PENDING_TOKEN_DEPOSIT, POOL_KEY_TO_VLP,
        POSITION_TOKEN_CONTRACT, STATE, TOKEN_TO_ESCROW, VLP_TO_LP_SHARES, VLP_TO_LP_TOKEN,
    },
};

/// Returns true if `msg` is a pool-related variant that pool_factory now owns
/// (Slice 1 starts with `RequestPoolCreation`; Slice 2 adds `AddLiquidity`;
/// Slice 3 adds `RemoveLiquidity`; Slice 4 adds
/// `RequestConcentratedPoolCreation`; later slices extend this set).
fn is_pool_variant(msg: &RouterCrossChainExecuteMsg) -> bool {
    matches!(
        msg,
        RouterCrossChainExecuteMsg::RequestPoolCreation { .. }
            | RouterCrossChainExecuteMsg::AddLiquidity { .. }
            | RouterCrossChainExecuteMsg::RemoveLiquidity(_)
            | RouterCrossChainExecuteMsg::RequestConcentratedPoolCreation(_)
    )
}

pub fn reusable_internal_ack_call(
    deps: &mut DepsMut,
    env: Env,
    msg: RouterCrossChainExecuteMsg,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    // When pool_factory owns pool flows, route pool-related acks to it.
    if crate::execute::proxy::pool_factory_is_initialised(deps)? && is_pool_variant(&msg) {
        let submsg = crate::execute::proxy::pool_factory_on_pool_ack_submsg(
            deps,
            to_json_binary(&msg)?,
            ack,
            is_native,
        )?;
        return Ok(Response::new()
            .add_attribute("method", "forward_pool_ack_to_pool_factory")
            .add_submessage(submsg));
    }
    // Parse the ack based on request
    match msg {
        RouterCrossChainExecuteMsg::RequestPoolCreation { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<AddLiquidityResponse> = from_json(ack)?;

            ack_pool_creation(deps.branch(), env, sender.address, res, tx_id, is_native)
        }
        RouterCrossChainExecuteMsg::RequestConcentratedPoolCreation(msg) => {
            let res: AcknowledgementMsg<ConcentratedAddLiquidityResponse> = from_json(ack)?;
            ack_concentrated_pool_creation(deps.branch(), env, msg, res, is_native)
        }

        RouterCrossChainExecuteMsg::RegisterDenom { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<RegisterDenomResponse> = from_json(ack)?;

            ack_register_denom(deps.branch(), env, sender.address, res, tx_id, is_native)
        }
        RouterCrossChainExecuteMsg::DeregisterDenom { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<DeregisterDenomResponse> = from_json(ack)?;

            ack_deregister_denom(deps.branch(), env, sender.address, res, tx_id, is_native)
        }

        RouterCrossChainExecuteMsg::AddLiquidity { tx_id, sender, .. } => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<AddLiquidityResponse> = from_json(ack)?;
            ack_add_liquidity(deps.branch(), res, sender.address, tx_id, is_native)
        }
        RouterCrossChainExecuteMsg::AddConcentratedLiquidity(msg) => {
            let res: AcknowledgementMsg<ConcentratedAddLiquidityResponse> = from_json(ack)?;
            ack_add_concentrated_liquidity(
                deps.branch(),
                res,
                msg.sender.address,
                msg.tx_id,
                is_native,
            )
        }
        RouterCrossChainExecuteMsg::RemoveLiquidity(msg) => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<RemoveLiquidityResponse> = from_json(ack)?;
            ack_remove_liquidity(deps.branch(), res, msg.sender.address, msg.tx_id, is_native)
        }
        RouterCrossChainExecuteMsg::RemoveConcentratedLiquidity(msg) => {
            let res: AcknowledgementMsg<ConcentratedRemoveLiquidityResponse> = from_json(ack)?;
            ack_remove_concentrated_liquidity(
                deps.branch(),
                res,
                msg.sender.address,
                msg.tx_id,
                is_native,
            )
        }
        RouterCrossChainExecuteMsg::CollectConcentratedFees(msg) => {
            let res: AcknowledgementMsg<ConcentratedCollectFeesResponse> = from_json(ack)?;
            ack_collect_concentrated_fees(
                deps.branch(),
                res,
                msg.sender.address,
                msg.tx_id,
                is_native,
            )
        }
        RouterCrossChainExecuteMsg::CollectConcentratedProtocolFees(msg) => {
            let res: AcknowledgementMsg<ConcentratedCollectProtocolFeesResponse> = from_json(ack)?;
            ack_collect_concentrated_protocol_fees(
                deps.branch(),
                res,
                msg.sender.address,
                msg.tx_id,
                is_native,
            )
        }
        RouterCrossChainExecuteMsg::Swap(swap) => {
            // Process acknowledgment for swap
            let res: AcknowledgementMsg<SwapResponse> = from_json(ack)?;
            ack_swap_request(
                deps.branch(),
                res,
                swap.sender.address,
                swap.tx_id,
                is_native,
            )
        }
        RouterCrossChainExecuteMsg::TransferVoucher(msg) => {
            let res: AcknowledgementMsg<TransferVoucherResponse> = from_json(ack)?;
            ack_transfer_request(
                deps.branch(),
                res,
                msg.sender.address,
                msg.token,
                msg.tx_id,
                is_native,
            )
        }
        RouterCrossChainExecuteMsg::DepositToken(deposit) => {
            // Process acknowledgment for deposit
            let res: AcknowledgementMsg<DepositTokenResponse> = from_json(ack)?;
            ack_deposit_token_request(
                deps.branch(),
                res,
                deposit.sender.address,
                deposit.tx_id,
                is_native,
            )
        }
    }
}

// Function to create pool
fn ack_pool_creation(
    deps: DepsMut,
    env: Env,
    sender: String,
    res: AcknowledgementMsg<AddLiquidityResponse>,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
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
            let admins = ADMIN.load(deps.storage)?;
            let escrow_code_id = state.escrow_code_id;
            let cw20_code_id = state.lp_code_id;

            PAIR_TO_VLP.save(
                deps.storage,
                existing_req.pair_info.get_pair()?.get_tupple(),
                &data.vlp_address.clone(),
            )?;
            // Prepare response
            let mut res = Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "pool_creation")
                .add_attribute("vlp", data.vlp_address.clone());
            // Collects PairInfo into a vector of Token Info for easy iteration
            let tokens = existing_req.pair_info.get_vec_token_info();
            for token in tokens {
                if token.token_type.is_voucher() {
                    continue;
                }
                let escrow_contract =
                    TOKEN_TO_ESCROW.may_load(deps.storage, token.token.clone())?;

                match escrow_contract {
                    Some(address) => {
                        let send_msg = token.token_type.create_escrow_msg(token.amount, address)?;
                        res = res.add_message(send_msg);
                    }
                    // Instantiate escrow if one doesn't exist
                    None => {
                        let init_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                            admin: Some(admins.migration_admin.clone().into_string()),
                            code_id: escrow_code_id,
                            msg: to_json_binary(&EscrowInstantiateMsg {
                                token_id: token.clone().token,
                                allowed_denom: Some(token.clone().token_type),
                            })?,
                            funds: vec![],
                            label: "escrow".to_string(),
                        });
                        PENDING_DEPOSIT_TOKEN.save(deps.storage, token.clone().token, &token)?;
                        res = res.add_submessage(SubMsg {
                            id: ESCROW_INSTANTIATE_REPLY_ID,
                            msg: init_msg,
                            gas_limit: None,
                            reply_on: ReplyOn::Always,
                            payload: Binary::default(),
                        });
                    }
                }
            }
            let lp_token_instantiate_data = existing_req.lp_token_instantiate_msg;
            // Instantiate cw20
            let init_cw20_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                admin: Some(admins.migration_admin.into_string()),
                code_id: cw20_code_id,
                msg: to_json_binary(&euclid::msgs::lp_token::msg::InstantiateMsg {
                    name: lp_token_instantiate_data.name,
                    symbol: lp_token_instantiate_data.symbol,
                    decimals: lp_token_instantiate_data.decimals,
                    initial_balances: vec![Cw20Coin {
                        amount: cosmwasm_std::Uint128::try_from(data.mint_lp_tokens)
                            .map_err(|e| ContractError::Std(e.into()))?,
                        address: data.sender.address,
                    }],
                    mint: lp_token_instantiate_data.mint,
                    marketing: lp_token_instantiate_data.marketing,
                    vlp: data.vlp_address.clone(),
                    factory: env.contract.address,
                    token_pair: existing_req.pair_info.get_pair()?,
                })?,
                funds: vec![],
                label: "cw20".to_string(),
            });
            // Save lp shares against vlp address
            VLP_TO_LP_SHARES.save(
                deps.storage,
                data.vlp_address.clone(),
                &Int256::from(
                    cosmwasm_std::Uint128::try_from(data.mint_lp_tokens)
                        .map_err(|e| ContractError::Std(e.into()))?,
                ),
            )?;

            Ok(res.add_submessage(SubMsg {
                id: LP_INSTANTIATE_REPLY_ID,
                msg: init_cw20_msg,
                gas_limit: None,
                reply_on: ReplyOn::Always,
                payload: Binary::default(),
            }))
        }

        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            // Refund tokens back to sender
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            for token_info in existing_req.pair_info.get_vec_token_info() {
                if token_info.token_type.is_voucher() {
                    continue;
                }
                let msg = token_info.token_type.create_transfer_msg(
                    token_info.amount,
                    sender.to_string(),
                    None,
                    None,
                )?;
                msgs.push(msg);
            }
            Ok(Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "reject_pool_request")
                .add_attribute("error", err.clone())
                .add_messages(msgs))
        }
    }
}

fn ack_concentrated_pool_creation(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainConcentratedRequestPoolCreationExecuteMsg,
    res: AcknowledgementMsg<ConcentratedAddLiquidityResponse>,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&msg.sender.address)?;
    let req_key = (sender.clone(), msg.tx_id.clone());
    let existing_req = PENDING_CONCENTRATED_POOL_REQUESTS
        .load(deps.storage, req_key.clone())
        .map_err(|_| ContractError::NotFound {
            msg: "pending concentrated pool request not found".to_string(),
        })?;

    PENDING_CONCENTRATED_POOL_REQUESTS.remove(deps.storage, req_key);

    match res {
        AcknowledgementMsg::Ok(data) => {
            POOL_KEY_TO_VLP.save(
                deps.storage,
                existing_req.pool_key.to_map_key(),
                &data.vlp_address,
            )?;

            let mut res = Response::new();
            for token_info in existing_req.pair_info.get_vec_token_info() {
                if token_info.token_type.is_voucher() {
                    continue;
                }

                let escrow_contract =
                    TOKEN_TO_ESCROW.may_load(deps.storage, token_info.token.clone())?;

                match escrow_contract {
                    Some(address) => {
                        let send_msg = token_info
                            .token_type
                            .create_escrow_msg(token_info.amount, address)?;
                        res = res.add_message(send_msg);
                    }
                    None => {
                        let state = STATE.load(deps.storage)?;
                        let admins = ADMIN.load(deps.storage)?;
                        let escrow_code_id = state.escrow_code_id;
                        let init_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                            admin: Some(admins.migration_admin.clone().into_string()),
                            code_id: escrow_code_id,
                            msg: to_json_binary(&EscrowInstantiateMsg {
                                token_id: token_info.clone().token,
                                allowed_denom: Some(token_info.clone().token_type),
                            })?,
                            funds: vec![],
                            label: "escrow".to_string(),
                        });
                        PENDING_DEPOSIT_TOKEN.save(
                            deps.storage,
                            token_info.clone().token,
                            &token_info,
                        )?;
                        res = res.add_submessage(SubMsg {
                            id: ESCROW_INSTANTIATE_REPLY_ID,
                            msg: init_msg,
                            gas_limit: None,
                            reply_on: ReplyOn::Always,
                            payload: Binary::default(),
                        });
                    }
                }
            }

            let position_token_contract = POSITION_TOKEN_CONTRACT
                .load(deps.storage)
                .map_err(|_| ContractError::new("Position token contract not registered"))?;

            let mint_msg = msgs::position_token::ExecuteMsg::Mint(position_token::MintMsg {
                token_id: data.position_id,
                token_info: position_token::TokenInfo {
                    owner: sender.clone(),
                    token_uri: None,
                },
                position_info: position_token::PositionInfo {
                    liquidity: data.liquidity_delta,
                    vlp_address: data.vlp_address.clone(),
                },
            });
            res = res.add_message(mint_msg.to_msg(position_token_contract)?);

            Ok(res
                .add_attribute("tx_id", msg.tx_id)
                .add_attribute("method", "ack_concentrated_pool_creation")
                .add_attribute("vlp", data.vlp_address)
                .add_attribute("position_id", data.position_id)
                .add_attribute("sender", sender))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            for token_info in existing_req.pair_info.get_vec_token_info() {
                if token_info.token_type.is_voucher() {
                    continue;
                }
                let msg = token_info.token_type.create_transfer_msg(
                    token_info.amount,
                    sender.to_string(),
                    None,
                    None,
                )?;
                msgs.push(msg);
            }
            Ok(Response::new()
                .add_attribute("tx_id", msg.tx_id)
                .add_attribute("method", "reject_concentrated_pool_request")
                .add_attribute("error", err)
                .add_messages(msgs))
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
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;

    // Remove pool request from MAP
    PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS.remove(deps.storage, req_key);

    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(_data) => {
            let state = STATE.load(deps.storage)?;
            let admins = ADMIN.load(deps.storage)?;
            let escrow_code_id = state.escrow_code_id;
            let token = existing_req.token;

            let existing_escrow = TOKEN_TO_ESCROW.may_load(deps.storage, token.token.clone())?;

            let mut response = Response::new()
                .add_attribute("tx_id", tx_id)
                .add_attribute("method", "ack_register_denom")
                .add_attribute("token", token.token.to_string())
                .add_attribute("token_type", token.token_type.get_key());

            if let Some(escrow_address) = existing_escrow {
                let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: escrow_address.into_string(),
                    msg: to_json_binary(&euclid::msgs::escrow::ExecuteMsg::AddAllowedDenom {
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
                    admin: Some(admins.migration_admin.into_string()),
                    code_id: escrow_code_id,
                    msg: to_json_binary(&EscrowInstantiateMsg {
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
                        payload: Binary::default(),
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
    res: AcknowledgementMsg<DeregisterDenomResponse>,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender, tx_id.clone());
    let existing_req = PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::PoolRequestDoesNotExists { req: tx_id.clone() })?;

    // Remove pool request from MAP
    PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS.remove(deps.storage, req_key);

    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(_data) => {
            let token = existing_req.token;

            let escrow_address = TOKEN_TO_ESCROW.load(deps.storage, token.token.clone())?;

            let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: escrow_address.into_string(),
                msg: to_json_binary(&euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
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
    let liquidity_info = PENDING_ADD_LIQUIDITY.load(deps.storage, req_key.clone())?;
    // Remove this from pending
    PENDING_ADD_LIQUIDITY.remove(deps.storage, req_key);
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove liquidity shares
            let shares = VLP_TO_LP_SHARES
                .may_load(deps.storage, data.vlp_address.clone())?
                .unwrap_or(Int256::zero());
            let shares = shares.checked_add(Int256::from(
                cosmwasm_std::Uint128::try_from(data.mint_lp_tokens)
                    .map_err(|e| ContractError::Std(e.into()))?,
            ))?;

            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_address.clone(), &shares)?;
            // Prepare response
            let mut res = Response::new().add_attribute("method", "ack_add_liquidity");

            // Send tokens back to escrow
            for token_info in liquidity_info.pair_info.get_vec_token_info() {
                // Vouchers are not escrowed
                if token_info.token_type.is_voucher() {
                    continue;
                }

                let liquidity = token_info.amount;
                let escrow_contract =
                    TOKEN_TO_ESCROW.load(deps.storage, token_info.token.clone())?;
                let send_msg = token_info
                    .token_type
                    .create_escrow_msg(liquidity, escrow_contract)?;
                res = res.add_message(send_msg);
            }

            // Mint cw20 tokens for sender //
            // Get cw20 contract address
            let lp_token_address = VLP_TO_LP_TOKEN.load(deps.storage, data.vlp_address)?;

            // Send mint msg
            let lp_mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_address.into_string(),
                msg: to_json_binary(&euclid::msgs::lp_token::msg::ExecuteMsg::Mint {
                    recipient: liquidity_info.sender,
                    amount: data.mint_lp_tokens,
                })?,
                funds: vec![],
            });

            Ok(res
                .add_message(lp_mint_msg)
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

fn ack_add_concentrated_liquidity(
    deps: DepsMut,
    res: AcknowledgementMsg<ConcentratedAddLiquidityResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let liquidity_info = PENDING_CONCENTRATED_ADD_LIQUIDITY
        .load(deps.storage, req_key.clone())
        .map_err(|_| ContractError::NotFound {
            msg: "pending concentrated liquidity request not found".to_string(),
        })?;
    PENDING_CONCENTRATED_ADD_LIQUIDITY.remove(deps.storage, req_key);

    match res {
        AcknowledgementMsg::Ok(data) => {
            let mut res = Response::new().add_attribute("method", "ack_add_concentrated_liquidity");
            let state = STATE.load(deps.storage)?;
            let admins = ADMIN.load(deps.storage)?;
            let escrow_code_id = state.escrow_code_id;
            for token_info in liquidity_info.pair_info.get_vec_token_info() {
                if token_info.token_type.is_voucher() {
                    continue;
                }
                let escrow_contract =
                    TOKEN_TO_ESCROW.may_load(deps.storage, token_info.token.clone())?;
                match escrow_contract {
                    Some(address) => {
                        let send_msg = token_info
                            .token_type
                            .create_escrow_msg(token_info.amount, address)?;
                        res = res.add_message(send_msg);
                    }
                    None => {
                        let init_msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                            admin: Some(admins.migration_admin.clone().into_string()),
                            code_id: escrow_code_id,
                            msg: to_json_binary(&EscrowInstantiateMsg {
                                token_id: token_info.clone().token,
                                allowed_denom: Some(token_info.clone().token_type),
                            })?,
                            funds: vec![],
                            label: "escrow".to_string(),
                        });
                        PENDING_DEPOSIT_TOKEN.save(
                            deps.storage,
                            token_info.clone().token,
                            &token_info,
                        )?;
                        res = res.add_submessage(SubMsg {
                            id: ESCROW_INSTANTIATE_REPLY_ID,
                            msg: init_msg,
                            gas_limit: None,
                            reply_on: ReplyOn::Always,
                            payload: Binary::default(),
                        });
                    }
                }
            }

            let position_token_contract = POSITION_TOKEN_CONTRACT
                .load(deps.storage)
                .map_err(|_| ContractError::new("Position token contract not registered"))?;

            let existing_position: Result<PositionInfoResponse, _> = deps.querier.query_wasm_smart(
                position_token_contract.clone(),
                &msgs::position_token::QueryMsg::PositionInfo {
                    token_id: data.position_id.to_string(),
                },
            );
            match existing_position {
                Ok(_position) => {
                    // Add liquidity to existing position
                    let update_position_msg = msgs::position_token::ExecuteMsg::UpdatePosition {
                        token_id: data.position_id,
                        liquidity_change: data.liquidity_delta.into(),
                    };
                    let update_position_msg =
                        update_position_msg.to_msg(position_token_contract)?;
                    res = res.add_message(update_position_msg);
                }
                Err(_err) => {
                    // Mint new position
                    let mint_msg =
                        msgs::position_token::ExecuteMsg::Mint(msgs::position_token::MintMsg {
                            token_id: data.position_id,
                            token_info: msgs::position_token::TokenInfo {
                                owner: sender.clone(),
                                token_uri: None,
                            },
                            position_info: msgs::position_token::PositionInfo {
                                liquidity: data.liquidity_delta,
                                vlp_address: data.vlp_address.clone(),
                            },
                        });
                    let mint_msg = mint_msg.to_msg(position_token_contract)?;
                    res = res.add_message(mint_msg);
                }
            }

            Ok(res
                .add_attribute("tx_id", tx_id)
                .add_attribute("position_id", data.position_id)
                .add_attribute("sender", sender))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
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
                )?;
                msgs.push(msg);
            }
            Ok(Response::new()
                .add_attribute("method", "concentrated_liquidity_tx_err_refund")
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
        .load(deps.storage, req_key.clone())
        .map_err(|_| ContractError::NotFound {
            msg: "pending remove liquidity request not found".to_string(),
        })?;
    // Remove this from pending
    PENDING_REMOVE_LIQUIDITY.remove(deps.storage, req_key.clone());
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove liquidity shares
            let shares = VLP_TO_LP_SHARES
                .may_load(deps.storage, data.vlp_address.clone())?
                .unwrap_or(Int256::zero());
            let shares = shares.checked_sub(Int256::from(
                cosmwasm_std::Uint128::try_from(data.burn_lp_tokens)
                    .map_err(|e| ContractError::Std(e.into()))?,
            ))?;

            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_address.clone(), &shares)?;
            // Prepare response
            let res = Response::new().add_attribute("method", "ack_remove_liquidity");

            // Burn cw20 tokens for sender //
            // Get cw20 contract address
            let lp_token_address = VLP_TO_LP_TOKEN.load(deps.storage, data.vlp_address)?;

            // Send burn msg
            let lp_burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: lp_token_address.into_string(),
                msg: to_json_binary(&euclid::msgs::lp_token::msg::ExecuteMsg::Burn {
                    amount: liquidity_info.lp_allocation,
                })?,
                funds: vec![],
            });

            Ok(res
                .add_message(lp_burn_msg)
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id))
        }

        AcknowledgementMsg::Error(err) => {
            // Its a native call so you can return error to reject complete execution call
            if is_native {
                return Err(ContractError::new(&err));
            }
            // Send back cw20 to original sender
            let lp_send_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: liquidity_info.lp_token.to_string(),
                msg: to_json_binary(&euclid::msgs::lp_token::msg::ExecuteMsg::Transfer {
                    recipient: sender.clone().into_string(),
                    amount: liquidity_info.lp_allocation,
                })?,
                funds: vec![],
            });
            Ok(Response::new()
                .add_message(lp_send_msg)
                .add_attribute("method", "liquidity_tx_err_refund")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("error", err))
        }
    }
}

/**
 * NOTE (M-04): This function is called when the remove liquidity request is acknowledged by the IBC.
 * It is used to give back the liquidity to the position.
 * This is needed because the remove liquidity request is sent to the IBC before the liquidity is removed from the position.
 * So, if the IBC fails, we need to give back the liquidity to the position.
 * It doesn't burn token because there might be a refund in progress while we are processing this packet.
 */
fn ack_remove_concentrated_liquidity(
    deps: DepsMut,
    res: AcknowledgementMsg<ConcentratedRemoveLiquidityResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let liquidity_info = PENDING_CONCENTRATED_REMOVE_LIQUIDITY
        .load(deps.storage, req_key.clone())
        .map_err(|_| ContractError::NotFound {
            msg: "pending concentrated remove liquidity request not found".to_string(),
        })?;
    PENDING_CONCENTRATED_REMOVE_LIQUIDITY.remove(deps.storage, req_key.clone());

    match res {
        AcknowledgementMsg::Ok(data) => {
            // We already have removed the liquidity from the position in the execute call.
            let mut res = Response::new()
                .add_attribute("method", "ack_remove_concentrated_liquidity")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("position_id", data.position_id.to_string())
                .add_attribute(
                    "liquidity_removed.amount_0",
                    data.liquidity_removed.token_1.amount.to_string(),
                )
                .add_attribute(
                    "liquidity_removed.amount_1",
                    data.liquidity_removed.token_2.amount.to_string(),
                )
                .add_attribute("liquidity_delta", data.liquidity_delta.to_string())
                .add_attribute("liquidity_after", data.liquidity_after.to_string())
                .add_attribute("vlp_address", data.vlp_address)
                .add_attribute("position_burned", data.position_burned.to_string());

            // Burn the position token only when the VLP has confirmed the
            // position is fully cleared (zero liquidity AND zero owed fees).
            if data.position_burned {
                let position_token_contract = POSITION_TOKEN_CONTRACT
                    .load(deps.storage)
                    .map_err(|_| ContractError::new("Position token contract not registered"))?;
                let burn_msg = msgs::position_token::ExecuteMsg::Burn {
                    token_id: data.position_id,
                };
                res = res.add_message(burn_msg.to_msg(position_token_contract)?);
            }

            Ok(res)
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            let position_token_contract = POSITION_TOKEN_CONTRACT
                .load(deps.storage)
                .map_err(|_| ContractError::new("Position token contract not registered"))?;

            // Give back the liquidity to the position
            let update_position_msg = msgs::position_token::ExecuteMsg::UpdatePosition {
                token_id: Uint128::new(liquidity_info.position_id),
                liquidity_change: liquidity_info.liquidity_delta.into(),
            };
            let update_position_msg = update_position_msg.to_msg(position_token_contract)?;
            Ok(Response::new()
                .add_attribute("method", "concentrated_remove_liquidity_err_refund")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("position_id", liquidity_info.position_id.to_string())
                .add_attribute("error", err)
                .add_message(update_position_msg))
        }
    }
}

fn ack_collect_concentrated_fees(
    deps: DepsMut,
    res: AcknowledgementMsg<ConcentratedCollectFeesResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let collect_info = PENDING_CONCENTRATED_COLLECT_FEES
        .may_load(deps.storage, req_key.clone())?
        .ok_or(ContractError::Generic {
            err: format!(
                "no pending collect fees request found for sender={}, tx_id={}",
                sender, tx_id
            ),
        })?;

    PENDING_CONCENTRATED_COLLECT_FEES.remove(deps.storage, req_key);

    match res {
        AcknowledgementMsg::Ok(data) => {
            ensure!(
                data.pool_key == collect_info.pool_key,
                ContractError::new("Pool key mismatch")
            );
            ensure!(
                data.position_id.u128() == collect_info.position_id,
                ContractError::new("Position id mismatch")
            );

            Ok(Response::new()
                .add_attribute("method", "ack_collect_concentrated_fees")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("position_id", data.position_id)
                .add_attribute("amount_0", data.amount_0)
                .add_attribute("amount_1", data.amount_1))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "ack_collect_concentrated_fees_error")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("position_id", collect_info.position_id.to_string())
                .add_attribute("error", err))
        }
    }
}

fn ack_collect_concentrated_protocol_fees(
    deps: DepsMut,
    res: AcknowledgementMsg<ConcentratedCollectProtocolFeesResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let collect_info = PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES
        .load(deps.storage, req_key.clone())
        .map_err(|_| ContractError::NotFound {
            msg: "pending concentrated collect protocol fees request not found".to_string(),
        })?;
    PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES.remove(deps.storage, req_key);

    match res {
        AcknowledgementMsg::Ok(data) => {
            ensure!(
                data.pool_key == collect_info.pool_key,
                ContractError::new("Pool key mismatch")
            );

            Ok(Response::new()
                .add_attribute("method", "ack_collect_concentrated_protocol_fees")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("amount_0", data.amount_0)
                .add_attribute("amount_1", data.amount_1))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "ack_collect_concentrated_protocol_fees_error")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("error", err))
        }
    }
}

// Function to process swap acknowledgment
fn ack_swap_request(
    deps: DepsMut,
    res: AcknowledgementMsg<SwapResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    // Validate that the pending swap exists for the sender
    let swap_info = PENDING_SWAPS.load(deps.storage, (sender.clone(), tx_id.clone()))?;
    // Remove this from pending swaps
    PENDING_SWAPS.remove(deps.storage, (sender.clone(), tx_id.clone()));
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
                let mut fee_state = FEE_STATE.load(deps.storage)?;

                // Add partner fee collected to the total
                fee_state
                    .partner_fees_collected
                    .add_fee(asset_in.token.to_string(), swap_info.partner_fee_amount);

                // Save new total partner fees collected to state
                FEE_STATE.save(deps.storage, &fee_state)?;
            }
            if !asset_in.token_type.is_voucher() {
                let escrow_address = TOKEN_TO_ESCROW.load(deps.storage, asset_in.token.clone())?;
                let send_msg = asset_in.create_escrow_msg(swap_info.amount_in, escrow_address)?;
                response = response.add_message(send_msg);

                // if partner fee is not zero, send it to the partner fee recipient
                if !swap_info.partner_fee_amount.is_zero() {
                    let partner_send_msg = asset_in.create_transfer_msg(
                        swap_info.partner_fee_amount,
                        swap_info.partner_fee_recipient.to_string(),
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
    let deposit_info = PENDING_TOKEN_DEPOSIT.load(deps.storage, (sender.clone(), tx_id.clone()))?;
    // Remove this from pending swaps
    PENDING_TOKEN_DEPOSIT.remove(deps.storage, (sender.clone(), tx_id.clone()));
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            let asset_in = deposit_info.asset_in.clone();

            // Get corresponding escrow
            let escrow_address = TOKEN_TO_ESCROW.load(deps.storage, asset_in.token.clone())?;

            let send_msg = asset_in.create_escrow_msg(data.amount, escrow_address)?;
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

fn ack_transfer_request(
    _deps: DepsMut,
    res: AcknowledgementMsg<TransferVoucherResponse>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{
        attr,
        testing::{mock_dependencies, mock_env},
        to_json_binary, Addr, Uint128, Uint256,
    };
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        deposit::DepositTokenRequest,
        liquidity::{AddLiquidityRequest, AddLiquidityResponse, RemoveLiquidityRequest},
        msgs::vlp::base::{DeregisterDenomResponse, PoolCreationResponse, RegisterDenomResponse},
        swap::{SwapRequest, TransferVoucherResponse},
        token::{
            Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
        },
    };
    use euclid_ibc::{
        ack::AcknowledgementMsg,
        router_ibc::{
            RouterCrossChainDepositTokenExecuteMsg, RouterCrossChainExecuteMsg,
            RouterCrossChainRemoveLiquidityExecuteMsg, RouterCrossChainSwapExecuteMsg,
            RouterCrossChainTransferVoucherExecuteMsg,
        },
    };

    use crate::{
        state::{
            DenomRegisterDeregisterRequest, PENDING_ADD_LIQUIDITY,
            PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS, PENDING_POOL_REQUESTS,
            PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, PENDING_TOKEN_DEPOSIT,
        },
        testing::helpers::{init, native_token, seed_escrow, TEST_CHAIN_UID},
    };

    // -----------------------------------------------------------------------
    // Shared helpers
    // -----------------------------------------------------------------------

    fn chain_uid() -> ChainUid {
        ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap()
    }

    fn cross_chain_user(addr: &str) -> CrossChainUser {
        CrossChainUser {
            chain_uid: chain_uid(),
            address: addr.to_string(),
        }
    }

    fn native_pair_with_denom_and_amount(
        token_a: &str,
        denom_a: &str,
        amount_a: u128,
        token_b: &str,
        denom_b: &str,
        amount_b: u128,
    ) -> PairWithDenomAndAmount {
        // Sort so token_1 <= token_2 (required by Pair)
        let (t1, d1, a1, t2, d2, a2) = if token_a <= token_b {
            (token_a, denom_a, amount_a, token_b, denom_b, amount_b)
        } else {
            (token_b, denom_b, amount_b, token_a, denom_a, amount_a)
        };
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create(t1.to_string()).unwrap(),
                amount: Uint256::from(a1),
                token_type: TokenType::Native {
                    denom: d1.to_string(),
                    decimals: None,
                },
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create(t2.to_string()).unwrap(),
                amount: Uint256::from(a2),
                token_type: TokenType::Native {
                    denom: d2.to_string(),
                    decimals: None,
                },
            },
        }
    }

    fn add_liquidity_msg(sender_addr: &str, tx_id: &str) -> RouterCrossChainExecuteMsg {
        RouterCrossChainExecuteMsg::AddLiquidity {
            sender: cross_chain_user(sender_addr),
            slippage_tolerance_bps: 50,
            pair: native_pair_with_denom_and_amount("aaa", "uaaa", 1000, "bbb", "ubbb", 1000),
            tx_id: tx_id.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // ack_pool_creation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ack_pool_creation_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: cross_chain_user(sender.as_str()),
            tx_id: "tx1".to_string(),
            pair: native_pair_with_denom_and_amount("aaa", "uaaa", 500, "bbb", "ubbb", 500),
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 50,
        };
        let ack = to_json_binary(&AcknowledgementMsg::Ok(AddLiquidityResponse {
            vlp_address: "vlp1".to_string(),
            tx_id: "tx1".to_string(),
            mint_lp_tokens: Uint256::from(100u128),
            sender: cross_chain_user(sender.as_str()),
        }))
        .unwrap();

        let err = reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false)
            .unwrap_err();

        assert!(matches!(
            err,
            ContractError::PoolRequestDoesNotExists { req } if req == "tx1"
        ));
    }

    #[test]
    fn test_ack_pool_creation_error_is_native_returns_err() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_native".to_string();
        let pair = native_pair_with_denom_and_amount("aaa", "uaaa", 500, "bbb", "ubbb", 500);

        PENDING_POOL_REQUESTS
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.clone()),
                &crate::state::PoolCreateRequest {
                    tx_id: tx_id.clone(),
                    sender: sender.clone(),
                    pair_info: pair.clone(),
                    lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg {
                        name: "LP".to_string(),
                        symbol: "LP".to_string(),
                        decimals: 6,
                        initial_balances: vec![],
                        mint: None,
                        marketing: None,
                    },
                },
            )
            .unwrap();

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: cross_chain_user(sender.as_str()),
            tx_id: tx_id.clone(),
            pair,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 50,
        };
        let ack = to_json_binary(&AcknowledgementMsg::<AddLiquidityResponse>::Error(
            "hub_err".to_string(),
        ))
        .unwrap();

        let err =
            reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, true).unwrap_err();

        assert_eq!(err, ContractError::new("hub_err"));
    }

    #[test]
    fn test_ack_pool_creation_error_not_native_returns_ok_with_reject_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_reject".to_string();
        let pair = native_pair_with_denom_and_amount("aaa", "uaaa", 500, "bbb", "ubbb", 500);

        PENDING_POOL_REQUESTS
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.clone()),
                &crate::state::PoolCreateRequest {
                    tx_id: tx_id.clone(),
                    sender: sender.clone(),
                    pair_info: pair.clone(),
                    lp_token_instantiate_msg: cw20_base::msg::InstantiateMsg {
                        name: "LP".to_string(),
                        symbol: "LP".to_string(),
                        decimals: 6,
                        initial_balances: vec![],
                        mint: None,
                        marketing: None,
                    },
                },
            )
            .unwrap();

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: cross_chain_user(sender.as_str()),
            tx_id: tx_id.clone(),
            pair,
            pool_config: euclid::msgs::vlp::base::PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 50,
        };
        let ack = to_json_binary(&AcknowledgementMsg::<AddLiquidityResponse>::Error(
            "fail".to_string(),
        ))
        .unwrap();

        let res =
            reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false).unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "reject_pool_request")));
    }

    // -----------------------------------------------------------------------
    // ack_register_denom tests
    // -----------------------------------------------------------------------

    fn seed_pending_denom_register(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        sender: &Addr,
        tx_id: &str,
        token: TokenWithDenom,
    ) {
        PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &DenomRegisterDeregisterRequest {
                    tx_id: tx_id.to_string(),
                    sender: sender.clone(),
                    token,
                },
            )
            .unwrap();
    }

    #[test]
    fn test_ack_register_denom_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let msg = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: cross_chain_user(sender.as_str()),
            tx_id: "no_such_tx".to_string(),
            token: native_token("aaa", "uaaa"),
        };
        let ack = to_json_binary(&AcknowledgementMsg::Ok(RegisterDenomResponse {})).unwrap();

        let err = reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false)
            .unwrap_err();

        assert!(matches!(
            err,
            ContractError::PoolRequestDoesNotExists { req } if req == "no_such_tx"
        ));
    }

    #[test]
    fn test_ack_register_denom_error_not_native_returns_ok_with_reject_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_rd_err";
        let token = native_token("aaa", "uaaa");
        seed_pending_denom_register(&mut deps, &sender, tx_id, token.clone());

        let msg = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: cross_chain_user(sender.as_str()),
            tx_id: tx_id.to_string(),
            token,
        };
        let ack = to_json_binary(&AcknowledgementMsg::<RegisterDenomResponse>::Error(
            "bad".to_string(),
        ))
        .unwrap();

        let res =
            reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false).unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "reject_denom_register")));
    }

    #[test]
    fn test_ack_register_denom_ok_with_existing_escrow_adds_message_no_submsg() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_rd_ok_existing";
        let token = native_token("aaa", "uaaa");

        // seed an existing escrow
        seed_escrow(&mut deps, "aaa", "escrow1");
        seed_pending_denom_register(&mut deps, &sender, tx_id, token.clone());

        let msg = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: cross_chain_user(sender.as_str()),
            tx_id: tx_id.to_string(),
            token,
        };
        let ack = to_json_binary(&AcknowledgementMsg::Ok(RegisterDenomResponse {})).unwrap();

        let res =
            reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false).unwrap();

        assert!(res.attributes.contains(&attr("create_escrow", "false")));
        assert_eq!(res.messages.len(), 1);
        // When escrow exists, a plain CosmosMsg (reply_on Never) is emitted, not a SubMsg reply
        assert_eq!(res.messages[0].reply_on, ReplyOn::Never);
    }

    #[test]
    fn test_ack_register_denom_ok_without_existing_escrow_adds_submsg() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_rd_ok_new";
        let token = native_token("aaa", "uaaa");

        // no escrow seeded
        seed_pending_denom_register(&mut deps, &sender, tx_id, token.clone());

        let msg = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: cross_chain_user(sender.as_str()),
            tx_id: tx_id.to_string(),
            token,
        };
        let ack = to_json_binary(&AcknowledgementMsg::Ok(RegisterDenomResponse {})).unwrap();

        let res =
            reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false).unwrap();

        assert!(res.attributes.contains(&attr("create_escrow", "true")));
        assert_eq!(res.messages.len(), 1);
        // When no escrow exists, an escrow instantiate SubMsg with reply_on Always is emitted
        assert_eq!(res.messages[0].reply_on, ReplyOn::Always);
    }

    // -----------------------------------------------------------------------
    // ack_deregister_denom tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_ack_deregister_denom_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let msg = RouterCrossChainExecuteMsg::DeregisterDenom {
            sender: cross_chain_user(sender.as_str()),
            tx_id: "no_tx".to_string(),
            token: native_token("aaa", "uaaa"),
        };
        let ack = to_json_binary(&AcknowledgementMsg::Ok(DeregisterDenomResponse {})).unwrap();

        let err = reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false)
            .unwrap_err();

        assert!(matches!(
            err,
            ContractError::PoolRequestDoesNotExists { req } if req == "no_tx"
        ));
    }

    #[test]
    fn test_ack_deregister_denom_error_not_native_returns_ok_with_reject_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_dd_err";
        let token = native_token("aaa", "uaaa");
        seed_pending_denom_register(&mut deps, &sender, tx_id, token.clone());

        let msg = RouterCrossChainExecuteMsg::DeregisterDenom {
            sender: cross_chain_user(sender.as_str()),
            tx_id: tx_id.to_string(),
            token,
        };
        let ack = to_json_binary(&AcknowledgementMsg::<DeregisterDenomResponse>::Error(
            "fail".to_string(),
        ))
        .unwrap();

        let res =
            reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false).unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "reject_denom_deregister")));
    }

    #[test]
    fn test_ack_deregister_denom_ok_returns_correct_attributes_and_message() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_dd_ok";
        let token = native_token("aaa", "uaaa");

        // Must have escrow in TOKEN_TO_ESCROW for the deregister Ok path
        seed_escrow(&mut deps, "aaa", "escrow1");
        seed_pending_denom_register(&mut deps, &sender, tx_id, token.clone());

        let msg = RouterCrossChainExecuteMsg::DeregisterDenom {
            sender: cross_chain_user(sender.as_str()),
            tx_id: tx_id.to_string(),
            token,
        };
        let ack = to_json_binary(&AcknowledgementMsg::Ok(DeregisterDenomResponse {})).unwrap();

        let res =
            reusable_internal_ack_call(&mut deps.as_mut(), mock_env(), msg, ack, false).unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "ack_deregister_denom")));
        assert_eq!(res.messages.len(), 1);
    }

    // -----------------------------------------------------------------------
    // ack_add_liquidity tests
    // -----------------------------------------------------------------------

    fn seed_pending_add_liquidity(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        sender: &Addr,
        tx_id: &str,
    ) {
        let pair = native_pair_with_denom_and_amount("aaa", "uaaa", 1000, "bbb", "ubbb", 1000);
        PENDING_ADD_LIQUIDITY
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &AddLiquidityRequest {
                    sender: sender.to_string(),
                    tx_id: tx_id.to_string(),
                    pair_info: pair,
                },
            )
            .unwrap();
    }

    #[test]
    fn test_ack_add_liquidity_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            add_liquidity_msg(sender.as_str(), "no_tx"),
            to_json_binary(&AcknowledgementMsg::Ok(
                euclid::liquidity::AddLiquidityResponse {
                    mint_lp_tokens: Uint256::zero(),
                    vlp_address: "vlp".to_string(),
                    tx_id: "no_tx".to_string(),
                    sender: cross_chain_user(sender.as_str()),
                },
            ))
            .unwrap(),
            false,
        );

        assert!(res.is_err());
    }

    #[test]
    fn test_ack_add_liquidity_error_is_native_returns_err() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_al_native";
        seed_pending_add_liquidity(&mut deps, &sender, tx_id);

        let ack = to_json_binary(
            &AcknowledgementMsg::<euclid::liquidity::AddLiquidityResponse>::Error(
                "hub_fail".to_string(),
            ),
        )
        .unwrap();

        let err = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            add_liquidity_msg(sender.as_str(), tx_id),
            ack,
            true,
        )
        .unwrap_err();

        assert_eq!(err, ContractError::new("hub_fail"));
    }

    #[test]
    fn test_ack_add_liquidity_error_not_native_returns_ok_with_refund_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_al_refund";
        seed_pending_add_liquidity(&mut deps, &sender, tx_id);

        let ack = to_json_binary(
            &AcknowledgementMsg::<euclid::liquidity::AddLiquidityResponse>::Error(
                "fail".to_string(),
            ),
        )
        .unwrap();

        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            add_liquidity_msg(sender.as_str(), tx_id),
            ack,
            false,
        )
        .unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "liquidity_tx_err_refund")));
    }

    // -----------------------------------------------------------------------
    // ack_remove_liquidity tests
    // -----------------------------------------------------------------------

    fn remove_liquidity_msg(sender_addr: &str, tx_id: &str) -> RouterCrossChainExecuteMsg {
        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        RouterCrossChainExecuteMsg::RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: cross_chain_user(sender_addr),
            lp_allocation: Uint256::from(50u128),
            pair,
            recipient: cross_chain_user(sender_addr),
            tx_id: tx_id.to_string(),
        })
    }

    fn seed_pending_remove_liquidity(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        sender: &Addr,
        tx_id: &str,
    ) {
        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        PENDING_REMOVE_LIQUIDITY
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &RemoveLiquidityRequest {
                    sender: sender.to_string(),
                    tx_id: tx_id.to_string(),
                    lp_allocation: Uint256::from(50u128),
                    pair,
                    lp_token: Addr::unchecked("lp_token_addr"),
                },
            )
            .unwrap();
    }

    #[test]
    fn test_ack_remove_liquidity_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            remove_liquidity_msg(sender.as_str(), "no_tx"),
            to_json_binary(&AcknowledgementMsg::<
                euclid::liquidity::RemoveLiquidityResponse,
            >::Error("x".to_string()))
            .unwrap(),
            false,
        );

        assert!(res.is_err());
    }

    #[test]
    fn test_ack_remove_liquidity_error_is_native_returns_err() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_rl_native";
        seed_pending_remove_liquidity(&mut deps, &sender, tx_id);

        let ack = to_json_binary(&AcknowledgementMsg::<
            euclid::liquidity::RemoveLiquidityResponse,
        >::Error("hub_fail".to_string()))
        .unwrap();

        let err = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            remove_liquidity_msg(sender.as_str(), tx_id),
            ack,
            true,
        )
        .unwrap_err();

        assert_eq!(err, ContractError::new("hub_fail"));
    }

    #[test]
    fn test_ack_remove_liquidity_error_not_native_returns_ok_with_refund_and_lp_transfer() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_rl_refund";
        seed_pending_remove_liquidity(&mut deps, &sender, tx_id);

        let ack = to_json_binary(&AcknowledgementMsg::<
            euclid::liquidity::RemoveLiquidityResponse,
        >::Error("fail".to_string()))
        .unwrap();

        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            remove_liquidity_msg(sender.as_str(), tx_id),
            ack,
            false,
        )
        .unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "liquidity_tx_err_refund")));
        assert_eq!(res.messages.len(), 1);
    }

    // -----------------------------------------------------------------------
    // ack_swap_request tests
    // -----------------------------------------------------------------------

    fn swap_msg(sender_addr: &str, tx_id: &str) -> RouterCrossChainExecuteMsg {
        RouterCrossChainExecuteMsg::Swap(RouterCrossChainSwapExecuteMsg {
            sender: cross_chain_user(sender_addr),
            asset_in: native_token("aaa", "uaaa"),
            amount_in: Uint256::from(100u128),
            asset_out: Token::create("bbb".to_string()).unwrap(),
            min_amount_out: Uint256::from(90u128),
            swaps: vec![],
            recipients: vec![],
            partner_fee_amount: Uint256::zero(),
            partner_fee_recipient: cross_chain_user(sender_addr),
            tx_id: tx_id.to_string(),
        })
    }

    fn voucher_swap_msg(sender_addr: &str, tx_id: &str) -> RouterCrossChainExecuteMsg {
        RouterCrossChainExecuteMsg::Swap(RouterCrossChainSwapExecuteMsg {
            sender: cross_chain_user(sender_addr),
            asset_in: crate::testing::helpers::voucher_token("aaa"),
            amount_in: Uint256::from(100u128),
            asset_out: Token::create("bbb".to_string()).unwrap(),
            min_amount_out: Uint256::from(90u128),
            swaps: vec![],
            recipients: vec![],
            partner_fee_amount: Uint256::zero(),
            partner_fee_recipient: cross_chain_user(sender_addr),
            tx_id: tx_id.to_string(),
        })
    }

    fn seed_pending_swap(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        sender: &Addr,
        tx_id: &str,
        asset_in: TokenWithDenom,
    ) {
        PENDING_SWAPS
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &SwapRequest {
                    sender: sender.to_string(),
                    tx_id: tx_id.to_string(),
                    asset_in,
                    amount_in: Uint256::from(100u128),
                    asset_out: Token::create("bbb".to_string()).unwrap(),
                    min_amount_out: Uint256::from(90u128),
                    swaps: vec![],
                    recipients: vec![],
                    partner_fee_amount: Uint256::zero(),
                    partner_fee_recipient: Addr::unchecked(sender.as_str()),
                },
            )
            .unwrap();
    }

    #[test]
    fn test_ack_swap_request_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            swap_msg(sender.as_str(), "no_tx"),
            to_json_binary(&AcknowledgementMsg::Ok(euclid::swap::SwapResponse {
                amount_out: Uint256::from(90u128),
                tx_id: "no_tx".to_string(),
            }))
            .unwrap(),
            false,
        );

        assert!(res.is_err());
    }

    #[test]
    fn test_ack_swap_request_error_is_native_returns_err() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_sw_native";
        seed_pending_swap(&mut deps, &sender, tx_id, native_token("aaa", "uaaa"));

        let ack = to_json_binary(&AcknowledgementMsg::<euclid::swap::SwapResponse>::Error(
            "hub_fail".to_string(),
        ))
        .unwrap();

        let err = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            swap_msg(sender.as_str(), tx_id),
            ack,
            true,
        )
        .unwrap_err();

        assert_eq!(err, ContractError::new("hub_fail"));
    }

    #[test]
    fn test_ack_swap_request_error_not_native_returns_ok_with_failed_swap_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_sw_fail";
        seed_pending_swap(&mut deps, &sender, tx_id, native_token("aaa", "uaaa"));

        let ack = to_json_binary(&AcknowledgementMsg::<euclid::swap::SwapResponse>::Error(
            "fail".to_string(),
        ))
        .unwrap();

        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            swap_msg(sender.as_str(), tx_id),
            ack,
            false,
        )
        .unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "process_failed_swap")));
    }

    #[test]
    fn test_ack_swap_request_ok_with_voucher_asset_in_no_escrow_msg() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_sw_ok_voucher";
        seed_pending_swap(
            &mut deps,
            &sender,
            tx_id,
            crate::testing::helpers::voucher_token("aaa"),
        );

        let ack = to_json_binary(&AcknowledgementMsg::Ok(euclid::swap::SwapResponse {
            amount_out: Uint256::from(90u128),
            tx_id: tx_id.to_string(),
        }))
        .unwrap();

        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            voucher_swap_msg(sender.as_str(), tx_id),
            ack,
            false,
        )
        .unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "process_successfull_swap")));
        // No CosmosMsg because asset_in is a voucher (not escrowed)
        assert!(res.messages.is_empty());
    }

    // -----------------------------------------------------------------------
    // ack_deposit_token_request tests
    // -----------------------------------------------------------------------

    fn deposit_msg(sender_addr: &str, tx_id: &str) -> RouterCrossChainExecuteMsg {
        RouterCrossChainExecuteMsg::DepositToken(RouterCrossChainDepositTokenExecuteMsg {
            sender: cross_chain_user(sender_addr),
            asset_in: native_token("aaa", "uaaa"),
            amount_in: Uint256::from(100u128),
            recipients: vec![],
            tx_id: tx_id.to_string(),
        })
    }

    fn seed_pending_deposit(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
        sender: &Addr,
        tx_id: &str,
    ) {
        PENDING_TOKEN_DEPOSIT
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.to_string()),
                &DepositTokenRequest {
                    sender: sender.to_string(),
                    tx_id: tx_id.to_string(),
                    asset_in: native_token("aaa", "uaaa"),
                    amount_in: Uint256::from(100u128),
                },
            )
            .unwrap();
    }

    #[test]
    fn test_ack_deposit_token_request_missing_pending_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            deposit_msg(sender.as_str(), "no_tx"),
            to_json_binary(&AcknowledgementMsg::Ok(
                euclid::deposit::DepositTokenResponse {
                    amount: Uint256::from(100u128),
                    token: Token::create("aaa".to_string()).unwrap(),
                    sender: cross_chain_user(sender.as_str()),
                },
            ))
            .unwrap(),
            false,
        );

        assert!(res.is_err());
    }

    #[test]
    fn test_ack_deposit_token_request_error_not_native_returns_ok_with_failed_deposit_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let tx_id = "tx_dt_fail";
        seed_pending_deposit(&mut deps, &sender, tx_id);

        let ack = to_json_binary(
            &AcknowledgementMsg::<euclid::deposit::DepositTokenResponse>::Error("fail".to_string()),
        )
        .unwrap();

        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            deposit_msg(sender.as_str(), tx_id),
            ack,
            false,
        )
        .unwrap();

        assert!(res
            .attributes
            .contains(&attr("method", "process_failed_deposit_token")));
    }

    // -----------------------------------------------------------------------
    // ack_transfer_request tests
    // -----------------------------------------------------------------------

    fn transfer_msg(sender_addr: &str, tx_id: &str) -> RouterCrossChainExecuteMsg {
        RouterCrossChainExecuteMsg::TransferVoucher(RouterCrossChainTransferVoucherExecuteMsg {
            sender: cross_chain_user(sender_addr),
            token: Token::create("aaa".to_string()).unwrap(),
            amount: Uint256::from(50u128),
            from: None,
            recipients: vec![],
            tx_id: tx_id.to_string(),
        })
    }

    #[test]
    fn test_ack_transfer_request_ok_returns_transfer_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let ack = to_json_binary(&AcknowledgementMsg::Ok(TransferVoucherResponse {
            token: Token::create("aaa".to_string()).unwrap(),
            tx_id: "tx_tr_ok".to_string(),
        }))
        .unwrap();

        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            transfer_msg(sender.as_str(), "tx_tr_ok"),
            ack,
            false,
        )
        .unwrap();

        assert!(res.attributes.contains(&attr("method", "transfer")));
    }

    #[test]
    fn test_ack_transfer_request_error_not_native_returns_transfer_error_method() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let ack = to_json_binary(&AcknowledgementMsg::<TransferVoucherResponse>::Error(
            "transfer_fail".to_string(),
        ))
        .unwrap();

        let res = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            transfer_msg(sender.as_str(), "tx_tr_err"),
            ack,
            false,
        )
        .unwrap();

        assert!(res.attributes.contains(&attr("method", "transfer_error")));
    }

    #[test]
    fn test_ack_transfer_request_error_is_native_returns_err() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("user1");
        let ack = to_json_binary(&AcknowledgementMsg::<TransferVoucherResponse>::Error(
            "native_fail".to_string(),
        ))
        .unwrap();

        let err = reusable_internal_ack_call(
            &mut deps.as_mut(),
            mock_env(),
            transfer_msg(sender.as_str(), "tx_tr_nat"),
            ack,
            true,
        )
        .unwrap_err();

        assert_eq!(err, ContractError::new("native_fail"));
    }
}
