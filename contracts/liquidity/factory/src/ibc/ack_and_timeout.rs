#[cfg(not(feature = "library"))]
use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, Int256, ReplyOn, Response,
    SubMsg, WasmMsg,
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
        escrow::InstantiateMsg as EscrowInstantiateMsg,
        vlp::base::{DeregisterDenomResponse, PoolCreationResponse, RegisterDenomResponse},
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
        pool_key_to_map_key, ADMIN, FEE_STATE, OWNER_POSITION_SET, PAIR_TO_VLP,
        PENDING_ADD_LIQUIDITY, PENDING_CONCENTRATED_ADD_LIQUIDITY,
        PENDING_CONCENTRATED_COLLECT_FEES, PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES,
        PENDING_CONCENTRATED_POOL_REQUESTS, PENDING_CONCENTRATED_REMOVE_LIQUIDITY,
        PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS, PENDING_DEPOSIT_TOKEN, PENDING_POOL_REQUESTS,
        PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, PENDING_TOKEN_DEPOSIT, POOL_KEY_TO_VLP,
        POSITION_ID_TO_METADATA, POSITION_TOKEN_CONTRACT, STATE, TOKEN_TO_ESCROW, VLP_TO_LP_SHARES,
        VLP_TO_LP_TOKEN,
    },
};

pub fn reusable_internal_ack_call(
    deps: &mut DepsMut,
    env: Env,
    msg: RouterCrossChainExecuteMsg,
    ack: Binary,
    is_native: bool,
) -> Result<Response, ContractError> {
    // Parse the ack based on request
    match msg {
        RouterCrossChainExecuteMsg::RequestPoolCreation { tx_id, sender, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<PoolCreationResponse> = from_json(ack)?;

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
    res: AcknowledgementMsg<PoolCreationResponse>,
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
                        amount: data.mint_lp_tokens,
                        address: data.sender.address,
                    }],
                    mint: lp_token_instantiate_data.mint,
                    marketing: lp_token_instantiate_data.marketing,
                    vlp: data.vlp_contract.clone(),
                    factory: env.contract.address,
                    token_pair: existing_req.pair_info.get_pair()?,
                })?,
                funds: vec![],
                label: "cw20".to_string(),
            });
            // Save lp shares against vlp address
            VLP_TO_LP_SHARES.save(deps.storage, data.vlp_contract, &data.mint_lp_tokens.into())?;

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
    let existing_req =
        match PENDING_CONCENTRATED_POOL_REQUESTS.may_load(deps.storage, req_key.clone())? {
            Some(req) => req,
            None => {
                return Ok(Response::new()
                    .add_attribute("method", "ack_concentrated_pool_creation_idempotent")
                    .add_attribute("tx_id", msg.tx_id));
            }
        };
    PENDING_CONCENTRATED_POOL_REQUESTS.remove(deps.storage, req_key);

    match res {
        AcknowledgementMsg::Ok(data) => {
            POOL_KEY_TO_VLP.save(
                deps.storage,
                pool_key_to_map_key(&existing_req.pool_key),
                &data.vlp_address,
            )?;

            let mut res = Response::new();
            for token_info in existing_req.pair_info.get_vec_token_info() {
                if token_info.token_type.is_voucher() {
                    continue;
                }

                let escrow_contract =
                    TOKEN_TO_ESCROW.load(deps.storage, token_info.token.clone())?;
                let send_msg = token_info
                    .token_type
                    .create_escrow_msg(token_info.amount, escrow_contract)?;
                res = res.add_message(send_msg);
            }

            POSITION_ID_TO_METADATA.save(
                deps.storage,
                data.position_id.u128(),
                &crate::state::ConcentratedPositionMetadata {
                    owner: sender.clone(),
                    pool_key: data.pool_key.clone(),
                    liquidity: data.liquidity_delta,
                    vlp_address: data.vlp_address.clone(),
                },
            )?;

            OWNER_POSITION_SET.save(
                deps.storage,
                (sender.clone(), data.position_id.u128()),
                &cosmwasm_std::Empty {},
            )?;

            if let Some(position_token_contract) = POSITION_TOKEN_CONTRACT.may_load(deps.storage)? {
                let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: position_token_contract.to_string(),
                    msg: to_json_binary(&euclid::msgs::position_token::ExecuteMsg::Mint {
                        token_id: data.position_id.to_string(),
                        owner: sender.to_string(),
                        token_uri: None,
                    })?,
                    funds: vec![],
                });
                res = res.add_message(mint_msg);
            }

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
            let shares = shares.checked_add(data.mint_lp_tokens.into())?;

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
    let liquidity_info =
        match PENDING_CONCENTRATED_ADD_LIQUIDITY.may_load(deps.storage, req_key.clone())? {
            Some(info) => info,
            None => {
                return Ok(Response::new()
                    .add_attribute("method", "ack_add_concentrated_liquidity_idempotent")
                    .add_attribute("tx_id", tx_id)
                    .add_attribute("sender", sender));
            }
        };
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

            let position_id = data.position_id.u128();
            let existing_meta = POSITION_ID_TO_METADATA.may_load(deps.storage, position_id)?;
            match existing_meta {
                Some(mut meta) => {
                    ensure!(meta.owner == sender, ContractError::Unauthorized {});
                    ensure!(
                        meta.pool_key == data.pool_key,
                        ContractError::new("Pool key mismatch")
                    );
                    meta.liquidity = meta.liquidity.checked_add(data.liquidity_delta)?;
                    meta.vlp_address = data.vlp_address.clone();
                    POSITION_ID_TO_METADATA.save(deps.storage, position_id, &meta)?;
                }
                None => {
                    POSITION_ID_TO_METADATA.save(
                        deps.storage,
                        position_id,
                        &crate::state::ConcentratedPositionMetadata {
                            owner: sender.clone(),
                            pool_key: data.pool_key.clone(),
                            liquidity: data.liquidity_delta,
                            vlp_address: data.vlp_address.clone(),
                        },
                    )?;

                    OWNER_POSITION_SET.save(
                        deps.storage,
                        (sender.clone(), position_id),
                        &cosmwasm_std::Empty {},
                    )?;

                    if let Some(position_token_contract) =
                        POSITION_TOKEN_CONTRACT.may_load(deps.storage)?
                    {
                        let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: position_token_contract.to_string(),
                            msg: to_json_binary(&euclid::msgs::position_token::ExecuteMsg::Mint {
                                token_id: data.position_id.to_string(),
                                owner: sender.to_string(),
                                token_uri: None,
                            })?,
                            funds: vec![],
                        });
                        res = res.add_message(mint_msg);
                    }
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
    let liquidity_info = PENDING_REMOVE_LIQUIDITY.load(deps.storage, req_key.clone())?;
    // Remove this from pending
    PENDING_REMOVE_LIQUIDITY.remove(deps.storage, req_key.clone());
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove liquidity shares
            let shares = VLP_TO_LP_SHARES
                .may_load(deps.storage, data.vlp_address.clone())?
                .unwrap_or(Int256::zero());
            let shares = shares.checked_sub(data.burn_lp_tokens.into())?;

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

fn ack_remove_concentrated_liquidity(
    deps: DepsMut,
    res: AcknowledgementMsg<ConcentratedRemoveLiquidityResponse>,
    sender: String,
    tx_id: String,
    is_native: bool,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&sender)?;
    let req_key = (sender.clone(), tx_id.clone());
    let liquidity_info =
        match PENDING_CONCENTRATED_REMOVE_LIQUIDITY.may_load(deps.storage, req_key.clone())? {
            Some(info) => info,
            None => {
                return Ok(Response::new()
                    .add_attribute("method", "ack_remove_concentrated_liquidity_idempotent")
                    .add_attribute("tx_id", tx_id)
                    .add_attribute("sender", sender));
            }
        };
    PENDING_CONCENTRATED_REMOVE_LIQUIDITY.remove(deps.storage, req_key.clone());

    match res {
        AcknowledgementMsg::Ok(data) => {
            if let Some(mut meta) =
                POSITION_ID_TO_METADATA.may_load(deps.storage, data.position_id.u128())?
            {
                meta.liquidity = meta.liquidity.checked_sub(data.liquidity_delta)?;
                if meta.liquidity.is_zero() {
                    POSITION_ID_TO_METADATA.remove(deps.storage, data.position_id.u128());
                    OWNER_POSITION_SET
                        .remove(deps.storage, (sender.clone(), data.position_id.u128()));
                    if let Some(position_token_contract) =
                        POSITION_TOKEN_CONTRACT.may_load(deps.storage)?
                    {
                        let burn_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: position_token_contract.to_string(),
                            msg: to_json_binary(&euclid::msgs::position_token::ExecuteMsg::Burn {
                                token_id: data.position_id.to_string(),
                            })?,
                            funds: vec![],
                        });
                        return Ok(Response::new()
                            .add_message(burn_msg)
                            .add_attribute("method", "ack_remove_concentrated_liquidity")
                            .add_attribute("tx_id", tx_id)
                            .add_attribute("position_id", data.position_id));
                    }
                } else {
                    POSITION_ID_TO_METADATA.save(deps.storage, data.position_id.u128(), &meta)?;
                }
            }
            Ok(Response::new()
                .add_attribute("method", "ack_remove_concentrated_liquidity")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("position_id", data.position_id))
        }
        AcknowledgementMsg::Error(err) => {
            if is_native {
                return Err(ContractError::new(&err));
            }
            Ok(Response::new()
                .add_attribute("method", "concentrated_remove_liquidity_err_refund")
                .add_attribute("sender", sender)
                .add_attribute("tx_id", tx_id)
                .add_attribute("position_id", liquidity_info.position_id.to_string())
                .add_attribute("error", err))
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
    let collect_info = PENDING_CONCENTRATED_COLLECT_FEES.load(deps.storage, req_key.clone())?;

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
    let collect_info =
        match PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES.may_load(deps.storage, req_key.clone())? {
            Some(info) => info,
            None => {
                return Ok(Response::new()
                    .add_attribute(
                        "method",
                        "ack_collect_concentrated_protocol_fees_idempotent",
                    )
                    .add_attribute("tx_id", tx_id)
                    .add_attribute("sender", sender));
            }
        };
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
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{Addr, Uint128};
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::liquidity::ConcentratedAddLiquidityResponse;
    use euclid::msgs::vlp::base::{PoolKey, PoolType};
    use euclid::token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenomAndAmount};
    use euclid_ibc::ack::AcknowledgementMsg;

    use crate::reply::ESCROW_INSTANTIATE_REPLY_ID;
    use crate::state::{
        ConcentratedAddLiquidityRequest, ADMIN, PENDING_CONCENTRATED_ADD_LIQUIDITY,
        PENDING_DEPOSIT_TOKEN, STATE, TOKEN_TO_ESCROW,
    };

    #[test]
    fn ack_add_concentrated_liquidity_instantiates_escrow_when_missing() {
        let mut deps = mock_dependencies();

        let sender = deps.api.addr_make("sender");
        let tx_id = "tx_1".to_string();

        let token_a = Token::create("tokena".to_string()).unwrap();
        let token_b = Token::create("tokenb".to_string()).unwrap();
        let pair = Pair::new(token_a.clone(), token_b.clone()).unwrap();

        // Set up factory state
        let state = crate::state::State {
            router_contract: "router".to_string(),
            relayer_contract: Addr::unchecked("relayer"),
            escrow_code_id: 42,
            lp_code_id: 2,
            chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            is_native: false,
        };
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN
            .save(
                deps.as_mut().storage,
                &EuclidAdmin::default(Addr::unchecked("admin")),
            )
            .unwrap();

        // Set up pending request with native tokens (not vouchers)
        let pair_info = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.clone(),
                amount: Uint128::new(1000),
                token_type: TokenType::Native {
                    denom: "utokena".to_string(),
                },
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.clone(),
                amount: Uint128::new(2000),
                token_type: TokenType::Native {
                    denom: "utokenb".to_string(),
                },
            },
        };

        let pool_key = PoolKey {
            pair: pair.clone(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 3000,
                tick_spacing: 60,
            },
        };

        PENDING_CONCENTRATED_ADD_LIQUIDITY
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.clone()),
                &ConcentratedAddLiquidityRequest {
                    tx_id: tx_id.clone(),
                    sender: sender.clone(),
                    pair_info,
                    pool_key: pool_key.clone(),
                    lower_tick_index: -600,
                    upper_tick_index: 600,
                    position_id: None,
                },
            )
            .unwrap();

        // Intentionally do NOT set up TOKEN_TO_ESCROW — this is the None case

        let ack_data = ConcentratedAddLiquidityResponse {
            pool_key,
            position_id: Uint128::new(1),
            liquidity_delta: Uint128::new(500),
            mint_lp_tokens: Uint128::zero(),
            vlp_address: "vlp_contract".to_string(),
            tx_id: tx_id.clone(),
            sender: CrossChainUser::new(
                ChainUid::create("testchain".to_string()).unwrap(),
                sender.to_string(),
            ),
        };

        let response = ack_add_concentrated_liquidity(
            deps.as_mut(),
            AcknowledgementMsg::Ok(ack_data),
            sender.to_string(),
            tx_id,
            false,
        )
        .unwrap();

        // Should have 2 submessages (one per token) to instantiate escrows
        let escrow_submsgs: Vec<_> = response
            .messages
            .iter()
            .filter(|m| m.id == ESCROW_INSTANTIATE_REPLY_ID)
            .collect();
        assert_eq!(
            escrow_submsgs.len(),
            2,
            "expected 2 escrow instantiation submessages, got {}",
            escrow_submsgs.len()
        );

        // Verify PENDING_DEPOSIT_TOKEN was saved for both tokens
        assert!(
            PENDING_DEPOSIT_TOKEN
                .may_load(deps.as_ref().storage, token_a)
                .unwrap()
                .is_some(),
            "expected pending deposit for token_a"
        );
        assert!(
            PENDING_DEPOSIT_TOKEN
                .may_load(deps.as_ref().storage, token_b)
                .unwrap()
                .is_some(),
            "expected pending deposit for token_b"
        );
    }

    #[test]
    fn ack_add_concentrated_liquidity_sends_to_existing_escrow() {
        let mut deps = mock_dependencies();

        let sender = deps.api.addr_make("sender");
        let tx_id = "tx_2".to_string();

        let token_a = Token::create("tokena".to_string()).unwrap();
        let token_b = Token::create("tokenb".to_string()).unwrap();
        let pair = Pair::new(token_a.clone(), token_b.clone()).unwrap();

        let state = crate::state::State {
            router_contract: "router".to_string(),
            relayer_contract: Addr::unchecked("relayer"),
            escrow_code_id: 42,
            lp_code_id: 2,
            chain_uid: ChainUid::create("testchain".to_string()).unwrap(),
            is_native: false,
        };
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN
            .save(
                deps.as_mut().storage,
                &EuclidAdmin::default(Addr::unchecked("admin")),
            )
            .unwrap();

        let pair_info = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.clone(),
                amount: Uint128::new(1000),
                token_type: TokenType::Native {
                    denom: "utokena".to_string(),
                },
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.clone(),
                amount: Uint128::new(2000),
                token_type: TokenType::Native {
                    denom: "utokenb".to_string(),
                },
            },
        };

        let pool_key = PoolKey {
            pair: pair.clone(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 3000,
                tick_spacing: 60,
            },
        };

        PENDING_CONCENTRATED_ADD_LIQUIDITY
            .save(
                deps.as_mut().storage,
                (sender.clone(), tx_id.clone()),
                &ConcentratedAddLiquidityRequest {
                    tx_id: tx_id.clone(),
                    sender: sender.clone(),
                    pair_info,
                    pool_key: pool_key.clone(),
                    lower_tick_index: -600,
                    upper_tick_index: 600,
                    position_id: None,
                },
            )
            .unwrap();

        // Set up existing escrows for both tokens
        TOKEN_TO_ESCROW
            .save(
                deps.as_mut().storage,
                token_a.clone(),
                &Addr::unchecked("escrow_a"),
            )
            .unwrap();
        TOKEN_TO_ESCROW
            .save(
                deps.as_mut().storage,
                token_b.clone(),
                &Addr::unchecked("escrow_b"),
            )
            .unwrap();

        let ack_data = ConcentratedAddLiquidityResponse {
            pool_key,
            position_id: Uint128::new(1),
            liquidity_delta: Uint128::new(500),
            mint_lp_tokens: Uint128::zero(),
            vlp_address: "vlp_contract".to_string(),
            tx_id: tx_id.clone(),
            sender: CrossChainUser::new(
                ChainUid::create("testchain".to_string()).unwrap(),
                sender.to_string(),
            ),
        };

        let response = ack_add_concentrated_liquidity(
            deps.as_mut(),
            AcknowledgementMsg::Ok(ack_data),
            sender.to_string(),
            tx_id,
            false,
        )
        .unwrap();

        // Should have 0 escrow instantiation submessages (escrows already exist)
        let escrow_submsgs: Vec<_> = response
            .messages
            .iter()
            .filter(|m| m.id == ESCROW_INSTANTIATE_REPLY_ID)
            .collect();
        assert_eq!(
            escrow_submsgs.len(),
            0,
            "expected 0 escrow instantiation submessages when escrows exist, got {}",
            escrow_submsgs.len()
        );

        // Should have 2 regular messages (send to existing escrows)
        let regular_msgs: Vec<_> = response
            .messages
            .iter()
            .filter(|m| m.id == 0)
            .collect();
        assert_eq!(
            regular_msgs.len(),
            2,
            "expected 2 send-to-escrow messages, got {}",
            regular_msgs.len()
        );
    }
}
