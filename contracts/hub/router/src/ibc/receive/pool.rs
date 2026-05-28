use cosmwasm_std::{
    ensure, to_json_binary, DepsMut, Env, Response, SubMsg, Uint128, Uint256, WasmMsg,
};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{simple_event, tx_event, TxType},
    fee::Fee,
    liquidity::{MAX_TICK, MIN_TICK},
    msgs::{
        self,
        virtual_balance::msg::{ExecuteApprove, ExecuteMint, ExecuteMsg as VirtualBalanceMsg},
        vlp::base::{
            PoolConfig, PoolKey, PoolType, VlpAddLiquidityMsg, VlpConcentratedAddLiquidityMsg,
            VlpConcentratedCollectFeesMsg, VlpConcentratedCollectProtocolFeesMsg,
            VlpConcentratedRegisterPoolMsg, VlpConcentratedRemoveLiquidityMsg, VlpRegisterPoolMsg,
            VlpRemoveLiquidityMsg,
        },
        vlp::concentrated::msg::ExecuteMsg as ConcentratedVlpExecuteMsg,
    },
    normalize::normalize_token_to_voucher,
    token::{PairWithDenomAndAmount, TokenMetadata},
    voucher::BalanceKey,
};
use euclid_ibc::router_ibc::{
    RouterCrossChainConcentratedAddLiquidityExecuteMsg,
    RouterCrossChainConcentratedCollectFeesExecuteMsg,
    RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg,
    RouterCrossChainConcentratedRemoveLiquidityExecuteMsg,
    RouterCrossChainRemoveLiquidityExecuteMsg, RouterCrossChainSingleSidedAddLiquidityMsg,
};

use crate::{
    query::{query_token_metadata_by_denom, query_token_status},
    reply::{
        ADD_LIQUIDITY_REPLY_ID, COLLECT_CONCENTRATED_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID,
        SINGLE_SIDED_SWAP_REPLY_ID, VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{
        get_clp_position_id, ADMIN, CLP_POSITION_ID_VLP_MAP, CONCENTRATED_FUNDS_INFO,
        CONCENTRATED_VLPS, FEE_STATE, FUNDS_INFO, PENDING_CONCENTRATED_COLLECT_FEES,
        PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES, PENDING_CONCENTRATED_REMOVE_LIQUIDITY,
        PENDING_REMOVE_LIQUIDITY, PENDING_SINGLE_SIDED_LIQUIDITY, STATE, VIRTUAL_BALANCE_CONTRACT,
        VLPS,
    },
};

fn default_aligned_tick_bounds(tick_spacing: u64) -> (i64, i64) {
    let spacing = tick_spacing as i64;
    let lower = (MIN_TICK / spacing) * spacing;
    let lower = if lower < MIN_TICK {
        lower + spacing
    } else {
        lower
    };
    let upper = (MAX_TICK / spacing) * spacing;
    (lower, upper)
}

pub fn ibc_execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    sender: CrossChainUser,
    pair_with_denom: PairWithDenomAndAmount,
    pool_config: PoolConfig,
    tx_id: String,
    slippage_tolerance_bps: u64,
    initial_tick: Option<i64>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admins = ADMIN.load(deps.storage)?;
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    let pair = pair_with_denom.get_pair()?;
    pair.validate()?;

    let mut response = Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::PoolCreation,
        ))
        .add_attribute("tx_id", tx_id.clone())
        .add_attribute("method", "request_pool_creation");

    let mut one_token_already_exists = false;

    for token in pair_with_denom.get_vec_token_info() {
        let token_registered = query_token_status(deps.as_ref(), &token.token)?;

        one_token_already_exists = one_token_already_exists || token_registered;

        // If its a voucher, then we need to check if this token atleast exist on one of the chains
        if token.token_type.is_voucher() {
            ensure!(
                token_registered,
                ContractError::new(
                    "Cannot create pool with voucher token that doesn't exist on any chain"
                )
            );
        } else {
            let token_registered_on_sender_chain = query_token_metadata_by_denom(
                deps.as_ref(),
                &virtual_balance_address,
                &token.token,
                &sender.chain_uid,
                &token.token_type,
            );
            match token_registered_on_sender_chain {
                Ok(token_metadata) => {
                    let token_decimals = token.token_type.get_decimals()?;
                    let metadata_decimals = token_metadata.token_type.get_decimals()?;
                    ensure!(
                        metadata_decimals == token_decimals,
                        ContractError::DecimalsMismatch {
                            expected: metadata_decimals,
                            received: token_decimals,
                        }
                    );
                }
                Err(_) => {
                    // We don't have this token registered on sender chain, so we need to register it. However we prevent new tokens if already the token id is registered on any chain
                    ensure!(
                        !token_registered,
                        ContractError::new("Token already registered on another chain")
                    );
                    let register_metadata_msg = VirtualBalanceMsg::RegisterTokenMetadata {
                        token_metadata: TokenMetadata::new(
                            token.token.clone(),
                            sender.chain_uid.clone(),
                            token.token_type.clone(),
                        ),
                    };
                    let register_metadata_wasm_msg = WasmMsg::Execute {
                        contract_addr: virtual_balance_address.to_string(),
                        msg: to_json_binary(&register_metadata_msg)?,
                        funds: vec![],
                    };
                    response = response.add_message(register_metadata_wasm_msg);

                    response = response.add_event(
                        simple_event()
                            .add_attribute("action", "register_denom")
                            .add_attribute("token", token.token.to_string())
                            .add_attribute("chain_uid", sender.chain_uid.to_string())
                            .add_attribute("token_type", token.token_type.get_key()),
                    );
                }
            };
        }
    }

    // Cannot create pool if both tokens are new
    ensure!(
        one_token_already_exists,
        ContractError::new("Cannot create pool with two new tokens")
    );

    let concentrated_pool_key = match pool_config {
        PoolConfig::Concentrated {
            fee_tier_bps,
            tick_spacing,
        } => Some(PoolKey {
            pair: pair.clone(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps,
                tick_spacing,
            },
        }),
        _ => None,
    };

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    if let Some(pool_key) = concentrated_pool_key {
        let tick_spacing = match pool_key.pool_type {
            PoolType::Concentrated { tick_spacing, .. } => tick_spacing,
            _ => 1,
        };
        let (lower_tick_index, upper_tick_index) = default_aligned_tick_bounds(tick_spacing);
        CONCENTRATED_FUNDS_INFO.save(
            deps.storage,
            &crate::state::ConcentratedFundsInfo {
                pair_with_denom: pair_with_denom.clone(),
                slippage_tolerance_bps,
                pool_key: pool_key.clone(),
                lower_tick_index,
                upper_tick_index,
                position_id: None,
                initial_tick,
            },
        )?;

        let existing_vlp = CONCENTRATED_VLPS.may_load(deps.storage, pool_key.to_map_key())?;
        let register_msg =
            ConcentratedVlpExecuteMsg::RegisterPool(VlpConcentratedRegisterPoolMsg {
                sender: sender.clone(),
                pool_key: pool_key.clone(),
                tx_id: tx_id.clone(),
            });
        if let Some(vlp_addr) = existing_vlp {
            let msg = WasmMsg::Execute {
                contract_addr: vlp_addr.to_string(),
                msg: to_json_binary(&register_msg)?,
                funds: vec![],
            };
            return Ok(
                response.add_submessage(SubMsg::reply_always(msg, VLP_POOL_REGISTER_REPLY_ID))
            );
        }

        let default_fee_recipient = FEE_STATE.load(deps.storage)?.default_fee_recipient;
        let default_fee_recipient = CrossChainUser::new(
            ChainUid::vsl_chain_uid()?,
            default_fee_recipient.to_string(),
        );
        let (fee_tier_bps, tick_spacing) = match pool_key.pool_type {
            PoolType::Concentrated {
                fee_tier_bps,
                tick_spacing,
            } => (fee_tier_bps, tick_spacing),
            _ => (500, 10),
        };
        let msg = WasmMsg::Instantiate {
            admin: Some(admins.general_admin.to_string()),
            code_id: state.concentrated_vlp_code_id,
            msg: to_json_binary(&msgs::vlp::concentrated::msg::InstantiateMsg {
                virtual_balance_contract: virtual_balance_address.clone(),
                pair: pair.clone(),
                fee: Fee::new(fee_tier_bps, 0, default_fee_recipient),
                execute: Some(msgs::vlp::concentrated::msg::ExecuteMsg::RegisterPool(
                    VlpConcentratedRegisterPoolMsg {
                        sender: sender.clone(),
                        pool_key,
                        tx_id: tx_id.clone(),
                    },
                )),
                admin: admins.general_admin,
                fee_tier_bps,
                tick_spacing,
                initial_tick,
            })?,
            funds: vec![],
            label: "Concentrated VLP".to_string(),
        };
        return Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID)));
    }

    FUNDS_INFO.save(
        deps.storage,
        &(pair_with_denom.clone(), slippage_tolerance_bps),
    )?;

    let register_msg = msgs::vlp::base::ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
        sender: sender.clone(),
        pair: pair.clone(),
        tx_id: tx_id.clone(),
    });

    let vlp = VLPS.may_load(deps.storage, pair.get_tupple())?;

    // If VLP exists, register pool on it, otherwise create new VLP contract
    if let Some(vlp_addr) = vlp {
        let msg = WasmMsg::Execute {
            contract_addr: vlp_addr.to_string(),
            msg: to_json_binary(&register_msg)?,
            funds: vec![],
        };
        return Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_POOL_REGISTER_REPLY_ID)));
    }

    let default_fee_recipient = FEE_STATE.load(deps.storage)?.default_fee_recipient;
    let default_fee_recipient = CrossChainUser::new(
        ChainUid::vsl_chain_uid()?,
        default_fee_recipient.to_string(),
    );
    let msg = match pool_config {
        PoolConfig::Stable { amp_factor } => WasmMsg::Instantiate {
            admin: Some(admins.general_admin.to_string()),
            code_id: state.stable_vlp_code_id,
            msg: to_json_binary(&msgs::vlp::stable::msg::InstantiateMsg {
                router: env.contract.address,
                virtual_balance_contract: virtual_balance_address.clone(),
                pair: pair.clone(),
                fee: Fee::new(10, 10, default_fee_recipient),
                execute: Some(msgs::vlp::stable::msg::ExecuteMsg::RegisterPool(
                    VlpRegisterPoolMsg {
                        sender: sender.clone(),
                        pair: pair.clone(),
                        tx_id: tx_id.clone(),
                    },
                )),
                admin: admins,
                amp_factor,
            })?,
            funds: vec![],
            label: "Stable VLP".to_string(),
        },
        PoolConfig::ConstantProduct {} => WasmMsg::Instantiate {
            admin: Some(admins.general_admin.to_string()),
            code_id: state.constant_product_vlp_code_id,
            msg: to_json_binary(&msgs::vlp::cp::msg::InstantiateMsg {
                router: env.contract.address,
                virtual_balance_contract: virtual_balance_address.clone(),
                pair: pair.clone(),
                fee: Fee::new(10, 10, default_fee_recipient),
                execute: Some(msgs::vlp::cp::msg::ExecuteMsg::RegisterPool(
                    VlpRegisterPoolMsg {
                        sender: sender.clone(),
                        pair: pair.clone(),
                        tx_id: tx_id.clone(),
                    },
                )),
                admin: admins,
            })?,
            funds: vec![],
            label: "Constant Product VLP".to_string(),
        },
        PoolConfig::Concentrated { .. } => {
            return Err(ContractError::new(
                "Use RequestConcentratedPoolCreation for concentrated pools",
            ))
        }
    };

    Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID)))
}

pub fn ibc_execute_add_liquidity(
    deps: DepsMut,
    sender: CrossChainUser,
    pair: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    tx_id: String,
) -> Result<Response, ContractError> {
    let vlp_address = VLPS.load(deps.storage, pair.get_pair()?.get_tupple())?;

    let mut response = Response::new().add_event(
        tx_event(&tx_id, &sender.to_sender_string(), TxType::AddLiquidity)
            .add_attribute("tx_id", tx_id.clone()),
    );

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    // Collect normalized amounts for each token
    let mut normalized_amounts: Vec<Uint256> = Vec::new();
    for token in pair.get_vec_token_info() {
        let normalized_amount = if token.token_type.is_voucher() {
            // Voucher tokens are already normalized
            token.amount
        } else {
            let metadata = query_token_metadata_by_denom(
                deps.as_ref(),
                &virtual_balance_address,
                &token.token,
                &sender.chain_uid,
                &token.token_type,
            )?;
            normalize_token_to_voucher(token.amount, metadata.token_type.get_decimals()?)?
        };
        normalized_amounts.push(normalized_amount);

        // Mint if not voucher token (escrow managed by virtual_balance)
        if !token.token_type.is_voucher() {
            // Mint virtual balance for the token
            let mint_virtual_balance_msg =
                euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(ExecuteMint {
                    amount: token.amount.into(),
                    balance_key: BalanceKey {
                        cross_chain_user: sender.clone(),
                        token_id: token.token.to_string(),
                    },
                    token_type: token.token_type.clone(),
                    token_source_chain_uid: sender.chain_uid.clone(),
                });

            let mint_virtual_balance_msg = WasmMsg::Execute {
                contract_addr: virtual_balance_address.to_string(),
                msg: to_json_binary(&mint_virtual_balance_msg)?,
                funds: vec![],
            };

            // Should reject full execution if failed
            response = response.add_message(mint_virtual_balance_msg);
        }

        // Transfer voucher token to the vlp contract
        let approve_voucher_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(ExecuteApprove {
                amount: normalized_amount,
                token_id: token.token.to_string(),
                spender: CrossChainUser::new(ChainUid::vsl_chain_uid()?, vlp_address.to_string()),
                owner: sender.clone(),
            });

        let approve_voucher_msg = WasmMsg::Execute {
            contract_addr: virtual_balance_address.to_string(),
            msg: to_json_binary(&approve_voucher_msg)?,
            funds: vec![],
        };

        // Should reject full execution if failed
        response = response.add_message(approve_voucher_msg);
    }

    let normalized_liquidity = pair
        .get_pair()?
        .get_pair_with_amount(normalized_amounts[0], normalized_amounts[1])?;

    let add_liquidity_msg = msgs::vlp::base::ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
        liquidity: normalized_liquidity,
        sender,
        tx_id,
        slippage_tolerance_bps,
    });

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&add_liquidity_msg)?,
        funds: vec![],
    };

    Ok(response.add_submessage(SubMsg::reply_always(msg, ADD_LIQUIDITY_REPLY_ID)))
}

#[allow(clippy::too_many_arguments)]
pub fn ibc_execute_request_concentrated_pool_creation(
    deps: DepsMut,
    env: Env,
    sender: CrossChainUser,
    pair_with_denom: PairWithDenomAndAmount,
    pool_key: PoolKey,
    tx_id: String,
    slippage_tolerance_bps: u64,
    initial_tick: Option<i64>,
) -> Result<Response, ContractError> {
    let (fee_tier_bps, tick_spacing) = match pool_key.pool_type {
        PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        } => (fee_tier_bps, tick_spacing),
        _ => return Err(ContractError::new("pool_key must be concentrated")),
    };

    ibc_execute_request_pool_creation(
        deps,
        env,
        sender,
        pair_with_denom,
        PoolConfig::Concentrated {
            fee_tier_bps,
            tick_spacing,
        },
        tx_id,
        slippage_tolerance_bps,
        initial_tick,
    )
}

pub fn ibc_execute_add_concentrated_liquidity(
    mut deps: DepsMut,
    msg: RouterCrossChainConcentratedAddLiquidityExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, msg.pool_key.to_map_key())?;

    let position_id = match msg.position_id {
        Some(id) => {
            // Validate existing position maps to this VLP
            let mapped_vlp = CLP_POSITION_ID_VLP_MAP
                .load(deps.storage, id.u128())
                .map_err(|_| ContractError::new("Position ID not found"))?;
            ensure!(
                mapped_vlp == vlp_address,
                ContractError::new("Position does not belong to this pool")
            );
            id
        }
        None => {
            // Generate new position ID and save mapping
            let new_id = get_clp_position_id(&mut deps)?;
            CLP_POSITION_ID_VLP_MAP.save(deps.storage, new_id, &vlp_address)?;
            Uint128::new(new_id)
        }
    };

    let mut response = Response::new().add_event(
        tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::AddLiquidity,
        )
        .add_attribute("tx_id", msg.tx_id.clone()),
    );

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    // Collect normalized amounts for each token
    let mut normalized_amounts: Vec<Uint256> = Vec::new();
    for token in msg.pair.get_vec_token_info() {
        let normalized_amount = if token.token_type.is_voucher() {
            token.amount
        } else {
            let metadata = query_token_metadata_by_denom(
                deps.as_ref(),
                &virtual_balance_address,
                &token.token,
                &msg.sender.chain_uid,
                &token.token_type,
            )?;
            normalize_token_to_voucher(token.amount, metadata.token_type.get_decimals()?)?
        };
        normalized_amounts.push(normalized_amount);

        if !token.token_type.is_voucher() {
            let mint_virtual_balance_msg =
                euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(ExecuteMint {
                    amount: token.amount,
                    balance_key: BalanceKey {
                        cross_chain_user: msg.sender.clone(),
                        token_id: token.token.to_string(),
                    },
                    token_type: token.token_type.clone(),
                    token_source_chain_uid: msg.sender.chain_uid.clone(),
                });

            response = response.add_message(WasmMsg::Execute {
                contract_addr: virtual_balance_address.to_string(),
                msg: to_json_binary(&mint_virtual_balance_msg)?,
                funds: vec![],
            });
        }

        let approve_voucher_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(ExecuteApprove {
                amount: normalized_amount,
                token_id: token.token.to_string(),
                spender: CrossChainUser::new(ChainUid::vsl_chain_uid()?, vlp_address.to_string()),
                owner: msg.sender.clone(),
            });

        response = response.add_message(WasmMsg::Execute {
            contract_addr: virtual_balance_address.to_string(),
            msg: to_json_binary(&approve_voucher_msg)?,
            funds: vec![],
        });
    }

    let normalized_liquidity = msg
        .pair
        .get_pair()?
        .get_pair_with_amount(normalized_amounts[0], normalized_amounts[1])?;

    let add_liquidity_msg =
        ConcentratedVlpExecuteMsg::AddLiquidity(VlpConcentratedAddLiquidityMsg {
            liquidity: normalized_liquidity,
            sender: msg.sender,
            tx_id: msg.tx_id,
            pool_key: msg.pool_key,
            lower_tick_index: msg.lower_tick_index,
            upper_tick_index: msg.upper_tick_index,
            position_id,
            slippage_tolerance_bps: msg.slippage_tolerance_bps,
        });

    let exec_msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&add_liquidity_msg)?,
        funds: vec![],
    };

    Ok(response.add_submessage(SubMsg::reply_always(exec_msg, ADD_LIQUIDITY_REPLY_ID)))
}

pub fn ibc_execute_remove_liquidity(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainRemoveLiquidityExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = VLPS.load(deps.storage, msg.pair.get_tupple())?;
    let response = Response::new()
        .add_event(tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::AddLiquidity,
        ))
        .add_attribute("tx_id", msg.tx_id.clone());

    let req_key = PENDING_REMOVE_LIQUIDITY.key(msg.tx_id.clone());
    ensure!(
        !req_key.has(deps.storage),
        ContractError::new("tx already present")
    );

    req_key.save(deps.storage, &msg)?;

    let remove_liquidity_msg =
        msgs::vlp::base::ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
            sender: msg.sender,
            lp_allocation: msg.lp_allocation,
            tx_id: msg.tx_id,
        });

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&remove_liquidity_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(msg, REMOVE_LIQUIDITY_REPLY_ID)))
}

pub fn ibc_execute_remove_concentrated_liquidity(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainConcentratedRemoveLiquidityExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, msg.pool_key.to_map_key())?;
    let response = Response::new()
        .add_event(tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::RemoveLiquidity,
        ))
        .add_attribute("tx_id", msg.tx_id.clone());

    let req_key = PENDING_CONCENTRATED_REMOVE_LIQUIDITY.key(msg.tx_id.clone());
    ensure!(
        !req_key.has(deps.storage),
        ContractError::new("tx already present")
    );

    req_key.save(deps.storage, &msg.clone())?;

    let remove_liquidity_msg =
        ConcentratedVlpExecuteMsg::RemoveLiquidity(VlpConcentratedRemoveLiquidityMsg {
            sender: msg.sender,
            pool_key: msg.pool_key,
            position_id: msg.position_id,
            liquidity_delta: msg.liquidity_delta,
            tx_id: msg.tx_id,
        });

    let exec_msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&remove_liquidity_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(exec_msg, REMOVE_LIQUIDITY_REPLY_ID)))
}

pub fn ibc_execute_collect_concentrated_fees(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainConcentratedCollectFeesExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, msg.pool_key.to_map_key())?;
    let response = Response::new()
        .add_event(tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::TransferVoucher,
        ))
        .add_attribute("tx_id", msg.tx_id.clone());

    let req_key = PENDING_CONCENTRATED_COLLECT_FEES.key(msg.tx_id.clone());
    ensure!(
        !req_key.has(deps.storage),
        ContractError::new("tx already present")
    );
    req_key.save(deps.storage, &msg.clone())?;

    let collect_msg = ConcentratedVlpExecuteMsg::CollectFees(VlpConcentratedCollectFeesMsg {
        sender: msg.sender,
        tx_id: msg.tx_id,
        pool_key: msg.pool_key,
        position_id: msg.position_id,
        recipient: msg.recipient,
    });
    let exec_msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&collect_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(
        exec_msg,
        COLLECT_CONCENTRATED_REPLY_ID,
    )))
}

pub fn ibc_execute_collect_concentrated_protocol_fees(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, msg.pool_key.to_map_key())?;
    let response = Response::new()
        .add_event(tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::TransferVoucher,
        ))
        .add_attribute("tx_id", msg.tx_id.clone());

    let req_key = PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES.key(msg.tx_id.clone());
    ensure!(
        !req_key.has(deps.storage),
        ContractError::new("tx already present")
    );
    req_key.save(deps.storage, &msg.clone())?;

    let collect_msg =
        ConcentratedVlpExecuteMsg::CollectProtocolFees(VlpConcentratedCollectProtocolFeesMsg {
            sender: msg.sender,
            tx_id: msg.tx_id,
            pool_key: msg.pool_key,
            recipient: msg.recipient,
            amount_0_requested: msg.amount_0_requested,
            amount_1_requested: msg.amount_1_requested,
        });
    let exec_msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&collect_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(
        exec_msg,
        COLLECT_CONCENTRATED_REPLY_ID,
    )))
}

// Single-sided add-liquidity entry point: validates the message, mints
// the user's virtual balance for the deposit token, approves the swap VLP,
// and kicks off the internal swap submsg. The swap reply will chain into
// add-liquidity on the same VLP.
pub fn ibc_execute_single_sided_add_liquidity(
    deps: DepsMut,
    _env: Env,
    msg: RouterCrossChainSingleSidedAddLiquidityMsg,
) -> Result<Response, ContractError> {
    msg.pair.validate()?;
    // asset_in must be one side of the target pair; the other side is the
    // swap output and the matching liquidity leg.
    ensure!(
        msg.asset_in.token == msg.pair.token_1 || msg.asset_in.token == msg.pair.token_2,
        ContractError::new("asset_in must be one of the pair tokens")
    );
    let asset_out = msg.pair.get_other_token(msg.asset_in.token.clone());

    // v1: single-hop only. Kept Vec<NextSwapPair> for forward-compat.
    ensure!(
        msg.swaps.len() == 1,
        ContractError::new("swap_route must contain exactly one hop in v1")
    );
    let hop = &msg.swaps[0];
    ensure!(
        hop.token_in == msg.asset_in.token,
        ContractError::new("swap_route first hop token_in must match asset_in")
    );
    ensure!(
        hop.token_out == asset_out,
        ContractError::new("swap_route last hop token_out must match the other pair token")
    );

    ensure!(!msg.amount_in.is_zero(), ContractError::ZeroAssetAmount {});
    ensure!(
        !msg.swap_amount.is_zero(),
        ContractError::ZeroAssetAmount {}
    );
    ensure!(
        msg.swap_amount < msg.amount_in,
        ContractError::new("swap_amount must be < amount_in")
    );
    ensure!(!msg.min_lp_out.is_zero(), ContractError::ZeroAssetAmount {});

    let req_key = msg.tx_id.clone();
    ensure!(
        !PENDING_SINGLE_SIDED_LIQUIDITY.has(deps.storage, req_key.clone()),
        ContractError::TxAlreadyExist {}
    );

    // Target VLP exists and is shared by both swap and add-liquidity legs.
    let vlp_address = VLPS
        .may_load(deps.storage, msg.pair.get_tupple())?
        .ok_or(ContractError::PoolDoesNotExist {})?;

    PENDING_SINGLE_SIDED_LIQUIDITY.save(deps.storage, req_key, &msg)?;

    let mut response = Response::new().add_event(
        tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::SingleSidedAddLiquidity,
        )
        .add_attribute("tx_id", msg.tx_id.clone()),
    );

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    // Resolve decimals via the asset's metadata on the sender's chain.
    let normalized_swap_amount = if msg.asset_in.token_type.is_voucher() {
        msg.swap_amount
    } else {
        let metadata = query_token_metadata_by_denom(
            deps.as_ref(),
            &virtual_balance_address,
            &msg.asset_in.token,
            &msg.sender.chain_uid,
            &msg.asset_in.token_type,
        )?;
        normalize_token_to_voucher(msg.swap_amount, metadata.token_type.get_decimals()?)?
    };

    // Mint virtual balance for the user. Virtual_balance internally normalizes
    // raw -> 24-dec voucher units, so we pass the raw amount_in here.
    if !msg.asset_in.token_type.is_voucher() {
        let mint_msg = VirtualBalanceMsg::Mint(ExecuteMint {
            amount: msg.amount_in.into(),
            balance_key: BalanceKey {
                cross_chain_user: msg.sender.clone(),
                token_id: msg.asset_in.token.to_string(),
            },
            token_type: msg.asset_in.token_type.clone(),
            token_source_chain_uid: msg.sender.chain_uid.clone(),
        });
        let mint_wasm_msg = WasmMsg::Execute {
            contract_addr: virtual_balance_address.to_string(),
            msg: to_json_binary(&mint_msg)?,
            funds: vec![],
        };
        response = response.add_message(mint_wasm_msg);
    }

    // Approve the swap VLP to spend the normalized swap_amount on behalf of the user.
    let approve_msg = VirtualBalanceMsg::Approve(ExecuteApprove {
        amount: normalized_swap_amount,
        token_id: msg.asset_in.token.to_string(),
        spender: CrossChainUser::new(ChainUid::vsl_chain_uid()?, vlp_address.to_string()),
        owner: msg.sender.clone(),
    });
    let approve_wasm_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&approve_msg)?,
        funds: vec![],
    };
    response = response.add_message(approve_wasm_msg);

    // Fire the swap submsg. min_token_out=0 — min_lp_out is the real guard,
    // checked at the add-liquidity reply.
    let swap_msg = msgs::vlp::base::ExecuteMsg::Swap(msgs::vlp::base::VlpSwapMsg {
        sender: msg.sender.clone(),
        tx_id: msg.tx_id.clone(),
        asset_in: msg.asset_in.token.clone(),
        amount_in: normalized_swap_amount,
        min_token_out: Uint256::zero(),
        next_swaps: vec![],
        test_fail: hop.test_fail,
    });
    let swap_wasm_msg = WasmMsg::Execute {
        contract_addr: vlp_address.to_string(),
        msg: to_json_binary(&swap_msg)?,
        funds: vec![],
    };

    Ok(response.add_submessage(SubMsg::reply_always(
        swap_wasm_msg,
        SINGLE_SIDED_SWAP_REPLY_ID,
    )))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::Uint256;
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        msgs::vlp::base::PoolConfig,
        token::{Pair, Token, TokenType},
    };
    use euclid_ibc::router_ibc::{
        RouterCrossChainExecuteMsg, RouterCrossChainRemoveLiquidityExecuteMsg,
    };

    use crate::{
        reply::REMOVE_LIQUIDITY_REPLY_ID,
        state::PENDING_REMOVE_LIQUIDITY,
        testing::{
            fixtures::initialized,
            helpers::{
                call_reusable, make_pool_pair, seed_virtual_balance, seed_vlp_aaa_bbb, MockDeps,
            },
        },
    };

    use rstest::*;

    #[rstest]
    fn test_ibc_add_liquidity_missing_vlp_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_virtual_balance(&mut initialized);
        // No VLPS entry

        let msg = RouterCrossChainExecuteMsg::AddLiquidity {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            slippage_tolerance_bps: 100,
            pair: make_pool_pair(50, 50),
            tx_id: "tx1".to_string(),
        };
        assert!(call_reusable(&mut initialized, msg, chain_uid).is_err());
    }

    // -----------------------------------------------------------------------
    // RemoveLiquidity dispatch
    // -----------------------------------------------------------------------

    fn make_remove_liquidity_msg(chain_uid: &ChainUid, tx_id: &str) -> RouterCrossChainExecuteMsg {
        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        RouterCrossChainExecuteMsg::RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            lp_allocation: Uint256::from(100u128),
            pair,
            recipient: CrossChainUser::new(chain_uid.clone(), "recipient".to_string()),
            tx_id: tx_id.to_string(),
        })
    }

    #[rstest]
    fn test_ibc_remove_liquidity_saves_pending_and_emits_submsg(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_vlp_aaa_bbb(&mut initialized);

        let res = call_reusable(
            &mut initialized,
            make_remove_liquidity_msg(&chain_uid, "tx_rem"),
            chain_uid,
        )
        .unwrap();

        assert_eq!(
            res.messages.last().unwrap().id,
            REMOVE_LIQUIDITY_REPLY_ID,
            "expected remove-liquidity submsg"
        );
        assert!(
            PENDING_REMOVE_LIQUIDITY.has(initialized.as_ref().storage, "tx_rem".to_string()),
            "expected PENDING_REMOVE_LIQUIDITY entry"
        );
    }

    #[rstest]
    fn test_ibc_remove_liquidity_duplicate_tx_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_vlp_aaa_bbb(&mut initialized);

        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        let pending = RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            lp_allocation: Uint256::from(100u128),
            pair,
            recipient: CrossChainUser::new(chain_uid.clone(), "recipient".to_string()),
            tx_id: "tx_dup".to_string(),
        };
        PENDING_REMOVE_LIQUIDITY
            .save(initialized.as_mut().storage, "tx_dup".to_string(), &pending)
            .unwrap();

        let result = call_reusable(
            &mut initialized,
            make_remove_liquidity_msg(&chain_uid, "tx_dup"),
            chain_uid,
        );
        assert_eq!(
            result.unwrap_err(),
            ContractError::new("tx already present")
        );
    }

    // -----------------------------------------------------------------------
    // RequestPoolCreation: new denom auto-registered in TOKEN_DENOMS
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_request_pool_creation_both_tokens_new_fails(mut initialized: MockDeps) {
        use cosmwasm_std::{
            from_json, to_json_binary, ContractResult, SystemError, SystemResult, WasmQuery,
        };
        use euclid::msgs::virtual_balance::msg::{
            GetTokenStatusResponse, QueryMsg as VirtualBalanceQueryMsg,
        };

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_virtual_balance(&mut initialized);
        // Neither token in TOKEN_DENOMS

        initialized.querier.update_wasm(|q| match q {
            WasmQuery::Smart { msg, .. } => {
                let parsed: VirtualBalanceQueryMsg = from_json(msg).unwrap();
                match parsed {
                    VirtualBalanceQueryMsg::GetTokenStatus { .. } => {
                        let resp = GetTokenStatusResponse { registered: false };
                        SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                    }
                    VirtualBalanceQueryMsg::GetTokenMetadataByDenom { .. } => {
                        SystemResult::Err(SystemError::InvalidRequest {
                            error: "metadata not found".to_string(),
                            request: Default::default(),
                        })
                    }
                    other => panic!("unexpected virtual_balance query: {other:?}"),
                }
            }
            _ => panic!("unexpected wasm query"),
        });

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            tx_id: "tx1".to_string(),
            pair: make_pool_pair(100, 100),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 100,
        };
        let result = call_reusable(&mut initialized, msg, chain_uid);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Cannot create pool with two new tokens"),
            "expected two-new-tokens error"
        );
    }

    #[rstest]
    fn test_ibc_request_pool_creation_token_registered_on_other_chain_fails(
        mut initialized: MockDeps,
    ) {
        use cosmwasm_std::{
            from_json, to_json_binary, ContractResult, SystemError, SystemResult, WasmQuery,
        };
        use euclid::msgs::virtual_balance::msg::{
            GetTokenStatusResponse, QueryMsg as VirtualBalanceQueryMsg,
        };

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_virtual_balance(&mut initialized);

        // Token "aaa" is registered globally but NOT on sender chain
        initialized.querier.update_wasm(|q| match q {
            WasmQuery::Smart { msg, .. } => {
                let parsed: VirtualBalanceQueryMsg = from_json(msg).unwrap();
                match parsed {
                    VirtualBalanceQueryMsg::GetTokenStatus { .. } => {
                        let resp = GetTokenStatusResponse { registered: true };
                        SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                    }
                    VirtualBalanceQueryMsg::GetTokenMetadataByDenom { .. } => {
                        SystemResult::Err(SystemError::InvalidRequest {
                            error: "metadata not found".to_string(),
                            request: Default::default(),
                        })
                    }
                    other => panic!("unexpected virtual_balance query: {other:?}"),
                }
            }
            _ => panic!("unexpected wasm query"),
        });

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            tx_id: "tx1".to_string(),
            pair: make_pool_pair(100, 100),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 100,
        };
        let result = call_reusable(&mut initialized, msg, chain_uid);
        assert_eq!(
            result.unwrap_err(),
            ContractError::new("Token already registered on another chain")
        );
    }

    // -----------------------------------------------------------------------
    // CLP AddConcentratedLiquidity: voucher normalization
    // -----------------------------------------------------------------------

    mod clp_normalization {
        use cosmwasm_std::{from_json, to_json_binary, CosmosMsg, Uint128, Uint256, WasmMsg};
        use euclid::{
            chain::ChainUid,
            cross_chain_user::CrossChainUser,
            msgs::{
                virtual_balance::msg::{ExecuteApprove, ExecuteMsg as VirtualBalanceExecuteMsg},
                vlp::{
                    base::{PoolKey, PoolType, VlpConcentratedAddLiquidityMsg},
                    concentrated::msg::ExecuteMsg as ConcentratedVlpExecuteMsg,
                },
            },
            token::{PairWithDenomAndAmount, Token, TokenType, TokenWithDenomAndAmount},
        };
        use euclid_ibc::router_ibc::RouterCrossChainConcentratedAddLiquidityExecuteMsg;

        use crate::{
            ibc::receive::pool::ibc_execute_add_concentrated_liquidity,
            state::ESCROW_BALANCES,
            testing::helpers::{
                make_clp_deps_with_decimals, make_pool_pair_with_decimals, seed_concentrated_vlp,
                MockDeps,
            },
        };

        fn test_pool_key(token_a: &str, token_b: &str) -> PoolKey {
            PoolKey {
                pair: euclid::token::Pair::new(
                    Token::create(token_a.to_string()).unwrap(),
                    Token::create(token_b.to_string()).unwrap(),
                )
                .unwrap(),
                pool_type: PoolType::Concentrated {
                    fee_tier_bps: 500,
                    tick_spacing: 10,
                },
            }
        }

        fn make_add_clp_msg(
            pair: PairWithDenomAndAmount,
            pool_key: PoolKey,
        ) -> RouterCrossChainConcentratedAddLiquidityExecuteMsg {
            RouterCrossChainConcentratedAddLiquidityExecuteMsg {
                sender: CrossChainUser::new(
                    ChainUid::create("chain1".to_string()).unwrap(),
                    "user".to_string(),
                ),
                pair,
                pool_key,
                lower_tick_index: -100,
                upper_tick_index: 100,
                position_id: None,
                slippage_tolerance_bps: 100,
                tx_id: "tx_clp".to_string(),
            }
        }

        fn extract_approve_amounts(
            deps: &mut MockDeps,
            msg: RouterCrossChainConcentratedAddLiquidityExecuteMsg,
        ) -> Vec<Uint256> {
            let res = ibc_execute_add_concentrated_liquidity(deps.as_mut(), msg).unwrap();
            res.messages
                .iter()
                .filter_map(|sub| match &sub.msg {
                    CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                        let parsed: VirtualBalanceExecuteMsg = from_json(msg).ok()?;
                        match parsed {
                            VirtualBalanceExecuteMsg::Approve(ExecuteApprove {
                                amount, ..
                            }) => Some(amount),
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .collect()
        }

        fn extract_vlp_liquidity_amounts(
            deps: &mut MockDeps,
            msg: RouterCrossChainConcentratedAddLiquidityExecuteMsg,
        ) -> (Uint256, Uint256) {
            let res = ibc_execute_add_concentrated_liquidity(deps.as_mut(), msg).unwrap();
            let vlp_msg = res.messages.last().unwrap();
            match &vlp_msg.msg {
                CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) => {
                    let parsed: ConcentratedVlpExecuteMsg = from_json(msg).unwrap();
                    match parsed {
                        ConcentratedVlpExecuteMsg::AddLiquidity(
                            VlpConcentratedAddLiquidityMsg { liquidity, .. },
                        ) => {
                            let tokens = liquidity.get_vec_token();
                            (tokens[0].amount, tokens[1].amount)
                        }
                        other => panic!("expected AddLiquidity, got {other:?}"),
                    }
                }
                other => panic!("expected WasmMsg::Execute, got {other:?}"),
            }
        }

        #[test]
        fn test_clp_add_liquidity_normalizes_6dec_amounts() {
            let pool_key = test_pool_key("aaa", "bbb");
            let mut deps =
                make_clp_deps_with_decimals(vec![("aaa".to_string(), 6), ("bbb".to_string(), 6)]);
            seed_concentrated_vlp(&mut deps, &pool_key, "clp_vlp");

            let pair = make_pool_pair_with_decimals(
                "aaa", "uaaa", 6, 1_000_000, "bbb", "ubbb", 6, 2_000_000,
            );

            let approves = extract_approve_amounts(&mut deps, make_add_clp_msg(pair, pool_key));
            // 1_000_000 * 10^18 = 10^24
            let expected_a = Uint256::from(1_000_000u128) * Uint256::from(10u128).pow(18);
            let expected_b = Uint256::from(2_000_000u128) * Uint256::from(10u128).pow(18);
            assert_eq!(approves.len(), 2);
            assert_eq!(approves[0], expected_a);
            assert_eq!(approves[1], expected_b);
        }

        #[test]
        fn test_clp_add_liquidity_normalizes_mixed_decimals() {
            let pool_key = test_pool_key("aaa", "bbb");
            let mut deps =
                make_clp_deps_with_decimals(vec![("aaa".to_string(), 6), ("bbb".to_string(), 18)]);
            seed_concentrated_vlp(&mut deps, &pool_key, "clp_vlp");

            let pair = make_pool_pair_with_decimals(
                "aaa",
                "uaaa",
                6,
                1_000_000,
                "bbb",
                "ubbb",
                18,
                1_000_000_000_000_000_000,
            );

            let approves = extract_approve_amounts(&mut deps, make_add_clp_msg(pair, pool_key));
            // 6-dec: 1_000_000 * 10^18
            let expected_a = Uint256::from(1_000_000u128) * Uint256::from(10u128).pow(18);
            // 18-dec: 10^18 * 10^6
            let expected_b = Uint256::from(10u128).pow(18) * Uint256::from(10u128).pow(6);
            assert_eq!(approves[0], expected_a);
            assert_eq!(approves[1], expected_b);
        }

        #[test]
        fn test_clp_add_liquidity_vlp_receives_normalized_amounts() {
            let pool_key = test_pool_key("aaa", "bbb");
            let mut deps =
                make_clp_deps_with_decimals(vec![("aaa".to_string(), 6), ("bbb".to_string(), 6)]);
            seed_concentrated_vlp(&mut deps, &pool_key, "clp_vlp");

            let pair =
                make_pool_pair_with_decimals("aaa", "uaaa", 6, 500_000, "bbb", "ubbb", 6, 500_000);

            let (amount_a, amount_b) =
                extract_vlp_liquidity_amounts(&mut deps, make_add_clp_msg(pair, pool_key));
            let expected = Uint256::from(500_000u128) * Uint256::from(10u128).pow(18);
            assert_eq!(amount_a, expected);
            assert_eq!(amount_b, expected);
        }

        #[test]
        fn test_clp_add_liquidity_voucher_tokens_skip_normalization() {
            let pool_key = test_pool_key("aaa", "bbb");
            // Set up deps but we won't query metadata for voucher tokens
            let mut deps = make_clp_deps_with_decimals(vec![]);
            seed_concentrated_vlp(&mut deps, &pool_key, "clp_vlp");

            let voucher_amount = Uint256::from(1_000_000_000_000_000_000_000_000u128);
            let pair = PairWithDenomAndAmount {
                token_1: TokenWithDenomAndAmount {
                    token: Token::create("aaa".to_string()).unwrap(),
                    token_type: TokenType::Voucher {},
                    amount: voucher_amount,
                },
                token_2: TokenWithDenomAndAmount {
                    token: Token::create("bbb".to_string()).unwrap(),
                    token_type: TokenType::Voucher {},
                    amount: voucher_amount,
                },
            };

            let approves = extract_approve_amounts(&mut deps, make_add_clp_msg(pair, pool_key));
            assert_eq!(approves[0], voucher_amount);
            assert_eq!(approves[1], voucher_amount);
        }

        #[test]
        #[allow(deprecated)]
        fn test_clp_add_liquidity_no_deprecated_escrow_writes() {
            let pool_key = test_pool_key("aaa", "bbb");
            let mut deps =
                make_clp_deps_with_decimals(vec![("aaa".to_string(), 6), ("bbb".to_string(), 6)]);
            seed_concentrated_vlp(&mut deps, &pool_key, "clp_vlp");

            let pair = make_pool_pair_with_decimals(
                "aaa", "uaaa", 6, 1_000_000, "bbb", "ubbb", 6, 1_000_000,
            );

            ibc_execute_add_concentrated_liquidity(deps.as_mut(), make_add_clp_msg(pair, pool_key))
                .unwrap();

            let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
            let key_a = ("aaa".to_string(), chain_uid.clone());
            let key_b = ("bbb".to_string(), chain_uid);
            assert!(
                ESCROW_BALANCES
                    .may_load(deps.as_ref().storage, key_a)
                    .unwrap()
                    .is_none(),
                "ESCROW_BALANCES should not be written by CLP path"
            );
            assert!(
                ESCROW_BALANCES
                    .may_load(deps.as_ref().storage, key_b)
                    .unwrap()
                    .is_none(),
                "ESCROW_BALANCES should not be written by CLP path"
            );
        }
    }

    // -----------------------------------------------------------------------
    // single_sided_add_liquidity
    // -----------------------------------------------------------------------

    mod single_sided {
        use super::*;
        use cosmwasm_std::{
            from_json, testing::mock_dependencies, to_json_binary, ContractResult, CosmosMsg,
            SystemResult, WasmMsg, WasmQuery,
        };
        use euclid::{
            msgs::virtual_balance::msg::{
                ExecuteMsg as VirtualBalanceMsg, GetTokenMetadataByDenomResponse,
                QueryMsg as VirtualBalanceQueryMsg,
            },
            swap::NextSwapPair,
            token::{TokenMetadata, TokenWithDenom},
        };
        use euclid_ibc::router_ibc::RouterCrossChainSingleSidedAddLiquidityMsg;

        use crate::{
            ibc::receive::pool::ibc_execute_single_sided_add_liquidity,
            reply::SINGLE_SIDED_SWAP_REPLY_ID,
            state::PENDING_SINGLE_SIDED_LIQUIDITY,
            testing::helpers::{init, TEST_VIRTUAL_BALANCE},
        };

        use cosmwasm_std::testing::{message_info, mock_env};

        // Build a deps instance with a metadata-responding querier (24 dec native).
        fn make_deps_with_metadata_querier() -> MockDeps {
            let mut deps = mock_dependencies();
            let creator = deps.api.addr_make("creator");
            init(deps.as_mut(), message_info(&creator, &[]));
            seed_virtual_balance(&mut deps);

            deps.querier.update_wasm(|q| match q {
                WasmQuery::Smart { contract_addr, msg }
                    if contract_addr == TEST_VIRTUAL_BALANCE =>
                {
                    let parsed: VirtualBalanceQueryMsg = from_json(msg).unwrap();
                    match parsed {
                        VirtualBalanceQueryMsg::GetTokenMetadataByDenom {
                            token_id,
                            chain_uid,
                            token_type,
                        } => {
                            let token_type_with_decimals = match token_type {
                                TokenType::Native { denom, .. } => TokenType::Native {
                                    denom,
                                    decimals: Some(24),
                                },
                                other => other,
                            };
                            let resp = GetTokenMetadataByDenomResponse {
                                metadata: TokenMetadata {
                                    token: Token::create(token_id).unwrap(),
                                    chain_uid,
                                    token_type: token_type_with_decimals,
                                    allowed: true,
                                },
                            };
                            SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                        }
                        other => panic!("unexpected vb query: {other:?}"),
                    }
                }
                _ => panic!("unexpected wasm query"),
            });
            deps
        }

        fn token_aaa() -> Token {
            Token::create("aaa".to_string()).unwrap()
        }
        fn token_bbb() -> Token {
            Token::create("bbb".to_string()).unwrap()
        }

        fn asset_in_native() -> TokenWithDenom {
            TokenWithDenom {
                token: token_aaa(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
            }
        }

        fn make_msg(
            amount_in: u128,
            swap_amount: u128,
            min_lp_out: u128,
            swaps: Vec<NextSwapPair>,
            asset_in: TokenWithDenom,
            pair: Pair,
            tx_id: &str,
        ) -> RouterCrossChainSingleSidedAddLiquidityMsg {
            let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
            RouterCrossChainSingleSidedAddLiquidityMsg {
                sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                asset_in,
                amount_in: Uint256::from(amount_in),
                swap_amount: Uint256::from(swap_amount),
                pair,
                swaps,
                min_lp_out: Uint256::from(min_lp_out),
                partner_fee_amount: Uint256::zero(),
                partner_fee_recipient: CrossChainUser::new(chain_uid, "user".to_string()),
                tx_id: tx_id.to_string(),
            }
        }

        fn pair_aaa_bbb() -> Pair {
            Pair::new(token_aaa(), token_bbb()).unwrap()
        }

        fn single_hop(token_in: Token, token_out: Token) -> Vec<NextSwapPair> {
            vec![NextSwapPair {
                token_in,
                token_out,
                pool_key: None,
                test_fail: None,
            }]
        }

        // ----- Happy path -----

        #[test]
        fn test_happy_path_saves_pending_and_emits_swap_submsg() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);

            let msg = make_msg(
                1000,
                400,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-ssl-1",
            );

            let res =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg.clone())
                    .unwrap();

            // Pending state is saved
            let pending = PENDING_SINGLE_SIDED_LIQUIDITY
                .load(deps.as_ref().storage, "tx-ssl-1".to_string())
                .unwrap();
            assert_eq!(pending.amount_in, Uint256::from(1000u128));
            assert_eq!(pending.swap_amount, Uint256::from(400u128));

            // Expect two non-sub messages: Mint(amount_in), Approve(normalized_swap_amount).
            // Then one submessage: VLP Swap with SINGLE_SIDED_SWAP_REPLY_ID.
            assert_eq!(res.messages.len(), 3);

            // The first two are mint + approve on virtual_balance.
            let mut saw_mint = false;
            let mut saw_approve = false;
            for sub in res.messages.iter().take(2) {
                if let CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr, msg, ..
                }) = &sub.msg
                {
                    assert_eq!(contract_addr, TEST_VIRTUAL_BALANCE);
                    let parsed: VirtualBalanceMsg = from_json(msg).unwrap();
                    match parsed {
                        VirtualBalanceMsg::Mint(m) => {
                            saw_mint = true;
                            // Raw amount_in flows into Mint
                            assert_eq!(m.amount, Uint256::from(1000u128));
                            assert_eq!(m.balance_key.token_id, "aaa");
                        }
                        VirtualBalanceMsg::Approve(a) => {
                            saw_approve = true;
                            // 24-dec native means normalize is identity
                            assert_eq!(a.amount, Uint256::from(400u128));
                            assert_eq!(a.token_id, "aaa");
                        }
                        other => panic!("unexpected vb msg: {other:?}"),
                    }
                } else {
                    panic!("expected wasm msg");
                }
            }
            assert!(saw_mint, "Expected Mint message");
            assert!(saw_approve, "Expected Approve message");

            // Final submessage: VLP Swap with reply id.
            let last = res.messages.last().unwrap();
            assert_eq!(last.id, SINGLE_SIDED_SWAP_REPLY_ID);
        }

        // ----- Validation rejections -----

        #[test]
        fn test_rejects_zero_hops() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            let msg = make_msg(
                1000,
                400,
                10,
                vec![],
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-zh",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(
                err,
                ContractError::new("swap_route must contain exactly one hop in v1")
            );
        }

        #[test]
        fn test_rejects_two_hops() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            let hops = vec![
                NextSwapPair {
                    token_in: token_aaa(),
                    token_out: token_bbb(),
                    pool_key: None,
                    test_fail: None,
                },
                NextSwapPair {
                    token_in: token_bbb(),
                    token_out: token_aaa(),
                    pool_key: None,
                    test_fail: None,
                },
            ];
            let msg = make_msg(
                1000,
                400,
                10,
                hops,
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-2h",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(
                err,
                ContractError::new("swap_route must contain exactly one hop in v1")
            );
        }

        #[test]
        fn test_rejects_hop_token_in_mismatch() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            // hop.token_in = ccc, but asset_in.token = aaa
            let ccc = Token::create("ccc".to_string()).unwrap();
            let msg = make_msg(
                1000,
                400,
                10,
                single_hop(ccc, token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-mismatch-in",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(
                err,
                ContractError::new("swap_route first hop token_in must match asset_in")
            );
        }

        #[test]
        fn test_rejects_hop_token_out_mismatch() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            let ccc = Token::create("ccc".to_string()).unwrap();
            let msg = make_msg(
                1000,
                400,
                10,
                single_hop(token_aaa(), ccc),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-mismatch-out",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(
                err,
                ContractError::new("swap_route last hop token_out must match the other pair token")
            );
        }

        #[test]
        fn test_rejects_zero_swap_amount() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            let msg = make_msg(
                1000,
                0,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-zsa",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(err, ContractError::ZeroAssetAmount {});
        }

        #[test]
        fn test_rejects_zero_amount_in() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            let msg = make_msg(
                0,
                0,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-zai",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(err, ContractError::ZeroAssetAmount {});
        }

        #[test]
        fn test_rejects_swap_amount_geq_amount_in() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            // swap_amount == amount_in
            let msg = make_msg(
                500,
                500,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-eq",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(err, ContractError::new("swap_amount must be < amount_in"));
        }

        #[test]
        fn test_rejects_zero_min_lp_out() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            let msg = make_msg(
                1000,
                400,
                0,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-zlp",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(err, ContractError::ZeroAssetAmount {});
        }

        #[test]
        fn test_rejects_asset_in_not_in_pair() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);
            // asset_in is `aaa`, but pair is (bbb, ccc) → asset_in not in pair.
            let ccc = Token::create("ccc".to_string()).unwrap();
            let bbb_ccc = Pair::new(token_bbb(), ccc.clone()).unwrap();
            let msg = make_msg(
                1000,
                400,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                bbb_ccc,
                "tx-not-in-pair",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(
                err,
                ContractError::new("asset_in must be one of the pair tokens")
            );
        }

        #[test]
        fn test_rejects_missing_vlp() {
            let mut deps = make_deps_with_metadata_querier();
            // Do NOT seed VLPS
            let msg = make_msg(
                1000,
                400,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-novlp",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(err, ContractError::PoolDoesNotExist {});
        }

        #[test]
        fn test_rejects_duplicate_tx_id() {
            let mut deps = make_deps_with_metadata_querier();
            seed_vlp_aaa_bbb(&mut deps);

            // Pre-seed PENDING_SINGLE_SIDED_LIQUIDITY with the tx_id we'll send.
            let existing = make_msg(
                1000,
                400,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-dup",
            );
            PENDING_SINGLE_SIDED_LIQUIDITY
                .save(deps.as_mut().storage, "tx-dup".to_string(), &existing)
                .unwrap();

            let msg = make_msg(
                1000,
                400,
                10,
                single_hop(token_aaa(), token_bbb()),
                asset_in_native(),
                pair_aaa_bbb(),
                "tx-dup",
            );
            let err =
                ibc_execute_single_sided_add_liquidity(deps.as_mut(), mock_env(), msg).unwrap_err();
            assert_eq!(err, ContractError::TxAlreadyExist {});
        }
    }
}
