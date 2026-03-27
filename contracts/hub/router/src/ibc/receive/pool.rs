use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, Response, SubMsg, Uint128, WasmMsg};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{register_denom_event, tx_event, TxType},
    fee::Fee,
    msgs::{
        self,
        router::TokenDenom,
        virtual_balance::msg::{ExecuteApprove, ExecuteMint},
        vlp::base::{
            PoolConfig, PoolKey, PoolType, VlpAddLiquidityMsg, VlpConcentratedAddLiquidityMsg,
            VlpConcentratedCollectFeesMsg, VlpConcentratedCollectProtocolFeesMsg,
            VlpConcentratedRegisterPoolMsg, VlpConcentratedRemoveLiquidityMsg, VlpRegisterPoolMsg,
            VlpRemoveLiquidityMsg,
        },
        vlp::concentrated::msg::ExecuteMsg as ConcentratedVlpExecuteMsg,
    },
    token::PairWithDenomAndAmount,
    voucher::BalanceKey,
};
use euclid_ibc::router_ibc::{
    RouterCrossChainConcentratedAddLiquidityExecuteMsg,
    RouterCrossChainConcentratedCollectFeesExecuteMsg,
    RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg,
    RouterCrossChainConcentratedRemoveLiquidityExecuteMsg,
    RouterCrossChainRemoveLiquidityExecuteMsg,
};

use crate::{
    reply::{
        ADD_LIQUIDITY_REPLY_ID, COLLECT_CONCENTRATED_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID,
        VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{
        pool_key_to_map_key, ADMIN, CONCENTRATED_FUNDS_INFO, CONCENTRATED_VLPS, ESCROW_BALANCES,
        FEE_STATE, FUNDS_INFO, PENDING_CONCENTRATED_COLLECT_FEES,
        PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES, PENDING_CONCENTRATED_REMOVE_LIQUIDITY,
        PENDING_REMOVE_LIQUIDITY, STATE, TOKEN_DENOMS, VIRTUAL_BALANCE_CONTRACT, VLPS,
    },
};

fn default_aligned_tick_bounds(tick_spacing: u64) -> (i64, i64) {
    const MIN_TICK: i64 = -887_272;
    const MAX_TICK: i64 = 887_272;
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
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admins = ADMIN.load(deps.storage)?;

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
        let mut registered_denoms = TOKEN_DENOMS
            .may_load(deps.storage, token.token.clone())?
            .unwrap_or_default();

        one_token_already_exists = one_token_already_exists || !registered_denoms.is_empty();

        // If its a voucher, then we need to check if this token atleast exist on one of the chains
        if token.token_type.is_voucher() {
            ensure!(
                !registered_denoms.is_empty(),
                ContractError::new(
                    "Cannot create pool with voucher token that doesn't exist on any chain"
                )
            );
        } else {
            let token_registered_on_sender_chain = registered_denoms.iter().any(|denom| {
                denom.chain_uid == sender.chain_uid && denom.token_type == token.token_type
            });
            // If its not a voucher, then this token must be present on sender chain with sent denom or its completely new token
            ensure!(
                registered_denoms.is_empty() || token_registered_on_sender_chain,
                ContractError::new(
                    format!(
                        "Token: {}:: sCannot use already existing denom without register first",
                        token.token
                    )
                    .as_str()
                )
            );
            // If its not a registered denom, lets register it now
            if !token_registered_on_sender_chain {
                registered_denoms.push(TokenDenom {
                    chain_uid: sender.chain_uid.clone(),
                    token_type: token.token_type.clone(),
                });
                TOKEN_DENOMS.save(deps.storage, token.token.clone(), &registered_denoms)?;
                response = response.add_event(register_denom_event(
                    &token.token,
                    &sender.chain_uid.to_string(),
                    &token.token_type,
                ));
            }
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
            },
        )?;

        let existing_vlp =
            CONCENTRATED_VLPS.may_load(deps.storage, pool_key_to_map_key(&pool_key))?;
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

    for token in pair.get_vec_token_info() {
        // Mint if not voucher token
        if !token.token_type.is_voucher() {
            // Increase Escrow balance
            let token_escrow_key = (token.token.to_string(), sender.chain_uid.clone());
            let token_escrow_balance = ESCROW_BALANCES
                .may_load(deps.storage, token_escrow_key.clone())?
                .unwrap_or(Uint128::zero());

            ESCROW_BALANCES.save(
                deps.storage,
                token_escrow_key,
                &token_escrow_balance.checked_add(token.amount)?,
            )?;

            // Mint virtual balance for the token
            let mint_virtual_balance_msg =
                euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(ExecuteMint {
                    amount: token.amount,
                    balance_key: BalanceKey {
                        cross_chain_user: sender.clone(),
                        token_id: token.token.to_string(),
                    },
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
                amount: token.amount,
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

    let add_liquidity_msg = msgs::vlp::base::ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
        liquidity: pair.get_pair_with_amount()?,
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
    )
}

pub fn ibc_execute_add_concentrated_liquidity(
    deps: DepsMut,
    msg: RouterCrossChainConcentratedAddLiquidityExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, pool_key_to_map_key(&msg.pool_key))?;

    let mut response = Response::new().add_event(
        tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::AddLiquidity,
        )
        .add_attribute("tx_id", msg.tx_id.clone()),
    );

    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    for token in msg.pair.get_vec_token_info() {
        if !token.token_type.is_voucher() {
            let token_escrow_key = (token.token.to_string(), msg.sender.chain_uid.clone());
            let token_escrow_balance = ESCROW_BALANCES
                .may_load(deps.storage, token_escrow_key.clone())?
                .unwrap_or(Uint128::zero());

            ESCROW_BALANCES.save(
                deps.storage,
                token_escrow_key,
                &token_escrow_balance.checked_add(token.amount)?,
            )?;

            let mint_virtual_balance_msg =
                euclid::msgs::virtual_balance::msg::ExecuteMsg::Mint(ExecuteMint {
                    amount: token.amount,
                    balance_key: BalanceKey {
                        cross_chain_user: msg.sender.clone(),
                        token_id: token.token.to_string(),
                    },
                });

            response = response.add_message(WasmMsg::Execute {
                contract_addr: virtual_balance_address.to_string(),
                msg: to_json_binary(&mint_virtual_balance_msg)?,
                funds: vec![],
            });
        }

        let approve_voucher_msg =
            euclid::msgs::virtual_balance::msg::ExecuteMsg::Approve(ExecuteApprove {
                amount: token.amount,
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

    let add_liquidity_msg =
        ConcentratedVlpExecuteMsg::AddLiquidity(VlpConcentratedAddLiquidityMsg {
            liquidity: msg.pair.get_pair_with_amount()?,
            sender: msg.sender,
            tx_id: msg.tx_id,
            pool_key: msg.pool_key,
            lower_tick_index: msg.lower_tick_index,
            upper_tick_index: msg.upper_tick_index,
            position_id: msg.position_id,
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
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, pool_key_to_map_key(&msg.pool_key))?;
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
            liquidity_delta: msg.lp_allocation,
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
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, pool_key_to_map_key(&msg.pool_key))?;
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
    let vlp_address = CONCENTRATED_VLPS.load(deps.storage, pool_key_to_map_key(&msg.pool_key))?;
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
