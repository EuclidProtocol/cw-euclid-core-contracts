#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcPacketReceiveMsg,
    IbcReceiveResponse, MessageInfo, Response, StdError, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    deposit::DepositTokenResponse,
    error::ContractError,
    events::{deregister_denom_event, register_denom_event, tx_event, TxType},
    fee::Fee,
    msgs::{
        self,
        router::{ExecuteMsg, TokenDenom},
        virtual_balance::{
            ExecuteApprove, ExecuteMint, ExecuteMsg as VirtualBalanceMsg, ExecuteTransfer,
        },
    },
    pool::{DeRegisterDenomResponse, PoolConfig, RegisterDenomResponse},
    swap::{TransferResponse, WithdrawResponse},
    token::{PairWithDenomAndAmount, TokenWithDenom},
    virtual_balance::BalanceKey,
};
use euclid_ibc::{
    ack::{make_ack_fail, AcknowledgementMsg},
    msg::{
        ChainIbcDepositTokenExecuteMsg, ChainIbcExecuteMsg, ChainIbcRemoveLiquidityExecuteMsg,
        ChainIbcSwapExecuteMsg, ChainIbcTransferExecuteMsg,
    },
};

use crate::{
    query::validate_swap_pairs,
    reply::{
        ADD_LIQUIDITY_REPLY_ID, IBC_RECEIVE_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID,
        VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{
        CHAIN_UID_TO_CHAIN, CHANNEL_TO_CHAIN_UID, DEREGISTERED_CHAINS, ESCROW_BALANCES, FUNDS_INFO,
        PENDING_REMOVE_LIQUIDITY, STATE, SWAP_ID_TO_MSG, TOKEN_DENOMS, VLPS,
    },
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    _deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    let internal_msg = ExecuteMsg::IbcCallbackReceive {
        receive_msg: msg.clone(),
    };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, IBC_RECEIVE_REPLY_ID);
    let msg: Result<ChainIbcExecuteMsg, StdError> = from_json(&msg.packet.data);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    Ok(
        IbcReceiveResponse::new(make_ack_fail("deafult_fail".to_string())?)
            .add_attribute("method", "ibc_packet_receive")
            .add_attribute("tx_id", tx_id)
            .add_submessage(sub_msg),
    )
}

pub fn ibc_receive_internal_call(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: IbcPacketReceiveMsg,
) -> Result<Response, ContractError> {
    ensure!(
        info.sender == env.contract.address,
        ContractError::Unauthorized {}
    );
    // Get the chain data from current channel received
    let channel = msg.packet.dest.channel_id;
    let chain_uid = CHANNEL_TO_CHAIN_UID.load(deps.storage, channel)?;
    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;

    // Ensure source port is the registered factory
    ensure!(
        msg.packet.src.port_id == format!("wasm.{address}", address = chain.factory),
        ContractError::Unauthorized {}
    );
    let msg: ChainIbcExecuteMsg = from_json(msg.packet.data)?;
    reusable_internal_call(deps, env, info, msg, chain_uid)
}

pub fn reusable_internal_call(
    deps: &mut DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ChainIbcExecuteMsg,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    let locked = STATE.load(deps.storage)?.locked;
    ensure!(!locked, ContractError::ContractLocked {});

    let deregistered_chains = DEREGISTERED_CHAINS
        .may_load(deps.storage)?
        .unwrap_or_default();
    ensure!(
        !deregistered_chains.contains(&chain_uid),
        ContractError::DeregisteredChain {}
    );
    let tx_id = msg.get_tx_id();

    let mut response = match msg {
        ChainIbcExecuteMsg::RequestPoolCreation {
            pair,
            sender,
            tx_id,
            slippage_tolerance_bps,
            pool_config,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            execute_request_pool_creation(
                deps.branch(),
                env,
                sender,
                pair,
                pool_config,
                tx_id,
                slippage_tolerance_bps,
            )?
        }
        ChainIbcExecuteMsg::RegisterDenom {
            token,
            sender,
            tx_id,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            execute_register_denom(deps.branch(), env, sender, token, tx_id)?
        }
        ChainIbcExecuteMsg::DeRegisterDenom {
            token,
            sender,
            tx_id,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            execute_deregister_denom(deps.branch(), env, sender, token, tx_id)?
        }
        ChainIbcExecuteMsg::AddLiquidity {
            slippage_tolerance_bps,
            pair,
            tx_id,
            sender,
            ..
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_add_liquidity(deps.branch(), sender, pair, slippage_tolerance_bps, tx_id)?
        }
        ChainIbcExecuteMsg::RemoveLiquidity(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_remove_liquidity(deps.branch(), env, msg)?
        }
        ChainIbcExecuteMsg::Swap(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_swap(deps.branch(), env, msg)?
        }
        ChainIbcExecuteMsg::Withdraw(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );

            let release_msg = ExecuteMsg::ReleaseEscrowInternal {
                sender: msg.sender,
                token: msg.token.clone(),
                amount: Some(msg.amount),
                cross_chain_addresses: msg.cross_chain_addresses,
                timeout: msg.timeout,
                tx_id: msg.tx_id.clone(),
            };

            Response::new()
                .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&release_msg)?,
                    funds: vec![],
                }))
                .set_data(to_json_binary(&AcknowledgementMsg::Ok(WithdrawResponse {
                    token: msg.token,
                    tx_id: msg.tx_id,
                }))?)
        }
        ChainIbcExecuteMsg::Transfer(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_transfer_virtual_balance(deps.branch(), env, msg)?
        }
        ChainIbcExecuteMsg::DepositToken(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );

            ibc_execute_deposit_token(deps.branch(), env, msg)?
        }
    };
    response = response.add_attribute("tx_id", tx_id);

    Ok(response)
}

fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    sender: CrossChainUser,
    pair_with_denom: PairWithDenomAndAmount,
    pool_config: PoolConfig,
    tx_id: String,
    slippage_tolerance_bps: u64,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

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

    FUNDS_INFO.save(
        deps.storage,
        &(pair_with_denom.clone(), slippage_tolerance_bps),
    )?;

    let register_msg = msgs::vlp::ExecuteMsg::RegisterPool {
        sender: sender.clone(),
        pair: pair.clone(),
        tx_id: tx_id.clone(),
    };

    let vlp = VLPS.may_load(deps.storage, pair.get_tupple())?;

    // If VLP exists, register pool on it, otherwise create new VLP contract
    if let Some(vlp_addr) = vlp {
        let msg = WasmMsg::Execute {
            contract_addr: vlp_addr,
            msg: to_json_binary(&register_msg)?,
            funds: vec![],
        };
        Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_POOL_REGISTER_REPLY_ID)))
    } else {
        let msg = match pool_config {
            PoolConfig::Stable { amp_factor } => WasmMsg::Instantiate {
                admin: Some(state.admin.clone()),
                code_id: state.stable_vlp_code_id,
                msg: to_json_binary(&msgs::stable_vlp::InstantiateMsg {
                    router: env.contract.address.to_string(),
                    virtual_balance: state
                        .virtual_balance_address
                        .ok_or(ContractError::Generic {
                            err: "virtual balance not instantiated".to_string(),
                        })?
                        .to_string(),
                    pair: pair.clone(),
                    fee: Fee::new(
                        10,
                        10,
                        CrossChainUser::new(ChainUid::vsl_chain_uid()?, state.admin.clone()),
                    ),
                    execute: Some(msgs::stable_vlp::ExecuteMsg::RegisterPool {
                        sender: sender.clone(),
                        pair: pair.clone(),
                        tx_id: tx_id.clone(),
                    }),
                    admin: state.admin.clone(),
                    amp_factor,
                })?,
                funds: vec![],
                label: "Stable VLP".to_string(),
            },
            PoolConfig::ConstantProduct {} => WasmMsg::Instantiate {
                admin: Some(state.admin.clone()),
                code_id: state.constant_product_vlp_code_id,
                msg: to_json_binary(&msgs::vlp::InstantiateMsg {
                    router: env.contract.address.to_string(),
                    virtual_balance: state
                        .virtual_balance_address
                        .ok_or(ContractError::Generic {
                            err: "virtual balance not instantiated".to_string(),
                        })?
                        .to_string(),
                    pair,
                    fee: Fee::new(
                        10,
                        10,
                        CrossChainUser::new(ChainUid::vsl_chain_uid()?, state.admin.clone()),
                    ),
                    execute: Some(register_msg),
                    admin: state.admin.clone(),
                })?,
                funds: vec![],
                label: "Constant Product VLP".to_string(),
            },
        };

        Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID)))
    }
}

fn execute_register_denom(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: TokenWithDenom,
    tx_id: String,
) -> Result<Response, ContractError> {
    println!("execute_register_denom");
    token.token.validate()?;

    let mut token_denoms = TOKEN_DENOMS
        .load(deps.storage, token.token.clone())
        .unwrap_or_default();

    let token_exists = token_denoms
        .iter()
        .any(|denom| denom.chain_uid == sender.chain_uid && denom.token_type == token.token_type);

    ensure!(!token_exists, ContractError::TokenAlreadyExist {});

    token_denoms.push(TokenDenom {
        chain_uid: sender.chain_uid.clone(),
        token_type: token.token_type.clone(),
    });
    println!("token key: {:?}", token.token);
    TOKEN_DENOMS.save(deps.storage, token.token.clone(), &token_denoms)?;

    let ack: AcknowledgementMsg<RegisterDenomResponse> =
        AcknowledgementMsg::Ok(RegisterDenomResponse {});

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::RegisterDenom,
        ))
        .add_event(register_denom_event(
            &token.token,
            &sender.chain_uid.to_string(),
            &token.token_type,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_register_denom")
        .set_data(to_json_binary(&ack)?))
}

fn execute_deregister_denom(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: TokenWithDenom,
    tx_id: String,
) -> Result<Response, ContractError> {
    token.token.validate()?;

    let mut token_denoms = TOKEN_DENOMS
        .load(deps.storage, token.token.clone())
        .unwrap_or_default();

    let token_exists = token_denoms
        .iter()
        .any(|denom| denom.chain_uid == sender.chain_uid && denom.token_type == token.token_type);

    ensure!(token_exists, ContractError::AssetDoesNotExist {});

    // Remove the denom from list
    token_denoms.retain(|denom| {
        denom.chain_uid != sender.chain_uid || denom.token_type != token.token_type
    });

    TOKEN_DENOMS.save(deps.storage, token.token.clone(), &token_denoms)?;

    let ack: AcknowledgementMsg<DeRegisterDenomResponse> =
        AcknowledgementMsg::Ok(DeRegisterDenomResponse {});

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::DeregisterDenom,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_deregister_denom")
        .add_event(deregister_denom_event(
            &token.token,
            &sender.chain_uid.to_string(),
            &token.token_type,
        ))
        .set_data(to_json_binary(&ack)?))
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

    let virtual_balance_address =
        STATE
            .load(deps.storage)?
            .virtual_balance_address
            .ok_or(ContractError::Generic {
                err: "virtual balance address doesn't exist".to_string(),
            })?;

    for token in pair.get_vec_token_info() {
        // Mint if not voucher token
        if !token.token_type.is_voucher() {
            // Increase Escrow balance
            let token_escrow_key = (token.token.clone(), sender.chain_uid.clone());
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
                euclid::msgs::virtual_balance::ExecuteMsg::Mint(ExecuteMint {
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
            euclid::msgs::virtual_balance::ExecuteMsg::Approve(ExecuteApprove {
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

    let add_liquidity_msg = msgs::vlp::ExecuteMsg::AddLiquidity {
        liquidity: pair.get_pair_with_amount()?,
        sender,
        tx_id,
        slippage_tolerance_bps,
    };

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address.clone(),
        msg: to_json_binary(&add_liquidity_msg)?,
        funds: vec![],
    };

    Ok(response.add_submessage(SubMsg::reply_always(msg, ADD_LIQUIDITY_REPLY_ID)))
}

fn ibc_execute_remove_liquidity(
    deps: DepsMut,
    _env: Env,
    msg: ChainIbcRemoveLiquidityExecuteMsg,
) -> Result<Response, ContractError> {
    let vlp_address = VLPS.load(deps.storage, msg.pair.get_tupple())?;
    let response = Response::new()
        .add_event(tx_event(
            &msg.tx_id,
            &msg.sender.to_sender_string(),
            TxType::AddLiquidity,
        ))
        .add_attribute("tx_id", msg.tx_id.clone());

    let req_key = PENDING_REMOVE_LIQUIDITY.key((
        msg.sender.chain_uid.clone(),
        msg.sender.address.clone(),
        msg.tx_id.clone(),
    ));
    ensure!(
        !req_key.has(deps.storage),
        ContractError::new("tx already present")
    );

    req_key.save(deps.storage, &msg)?;

    let remove_liquidity_msg = msgs::vlp::ExecuteMsg::RemoveLiquidity {
        sender: msg.sender,
        lp_allocation: msg.lp_allocation,
        tx_id: msg.tx_id,
    };

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address,
        msg: to_json_binary(&remove_liquidity_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(msg, REMOVE_LIQUIDITY_REPLY_ID)))
}

fn ibc_execute_swap(
    deps: DepsMut,
    _env: Env,
    msg: ChainIbcSwapExecuteMsg,
) -> Result<Response, ContractError> {
    let first_swap = msg.swaps.first().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    let last_swap = msg.swaps.last().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    ensure!(
        first_swap.token_in == msg.asset_in.token,
        ContractError::new("Asset IN does not match router")
    );

    ensure!(
        last_swap.token_out == msg.asset_out,
        ContractError::new("Asset OUT does not match router")
    );

    let req_key = (
        msg.sender.chain_uid.clone(),
        msg.sender.address.clone(),
        msg.tx_id.clone(),
    );

    ensure!(
        !SWAP_ID_TO_MSG.has(deps.storage, req_key.clone()),
        ContractError::TxAlreadyExist {}
    );

    SWAP_ID_TO_MSG.save(deps.storage, req_key, &msg)?;

    let mut response = Response::new().add_event(
        tx_event(&msg.tx_id, &msg.sender.to_sender_string(), TxType::Swap)
            .add_attribute("tx_id", msg.tx_id.clone()),
    );

    let sender = msg.sender;

    let virtual_balance_address =
        STATE
            .load(deps.storage)?
            .virtual_balance_address
            .ok_or(ContractError::Generic {
                err: "virtual balance address doesn't exist".to_string(),
            })?;

    let swap_vlps = validate_swap_pairs(deps.as_ref(), &msg.swaps);
    ensure!(
        swap_vlps.is_ok(),
        ContractError::Generic {
            err: "VLPS listed in swaps are not registered".to_string()
        }
    );
    let swap_vlps = swap_vlps?;

    let (first_swap, next_swaps) = swap_vlps.split_first().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    // Mint voucher token in escrow balance if it is not a voucher token
    if !msg.asset_in.token_type.is_voucher() {
        let token_escrow_key = (msg.asset_in.token.clone(), sender.chain_uid.clone());
        let token_escrow_balance = ESCROW_BALANCES
            .may_load(deps.storage, token_escrow_key.clone())?
            .unwrap_or(Uint128::zero());

        ESCROW_BALANCES.save(
            deps.storage,
            token_escrow_key,
            &token_escrow_balance.checked_add(msg.amount_in)?,
        )?;

        // Mint virtual balance for the first swap vlp so it can start processing tx
        let mint_virtual_balance_msg =
            euclid::msgs::virtual_balance::ExecuteMsg::Mint(ExecuteMint {
                amount: msg.amount_in,
                balance_key: BalanceKey {
                    cross_chain_user: sender.clone(),
                    token_id: msg.asset_in.token.to_string(),
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

    let approve_voucher_msg = euclid::msgs::virtual_balance::ExecuteMsg::Approve(ExecuteApprove {
        amount: msg.amount_in,
        token_id: msg.asset_in.token.to_string(),
        spender: CrossChainUser::new(ChainUid::vsl_chain_uid()?, first_swap.vlp_address.clone()),
        owner: sender.clone(),
    });

    let approve_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&approve_voucher_msg)?,
        funds: vec![],
    };

    // Should reject full execution if failed
    response = response.add_message(approve_voucher_msg);

    if msg.asset_in.token_type.is_voucher()
        && !msg.partner_fee_amount.is_zero()
        && msg.partner_fee_recipient != sender
    {
        let transfer_voucher_msg =
            euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
                amount: msg.partner_fee_amount,
                token_id: msg.asset_in.token.to_string(),
                from: sender.clone(),
                to: msg.partner_fee_recipient.clone(),
            });

        let transfer_voucher_msg = WasmMsg::Execute {
            contract_addr: virtual_balance_address.to_string(),
            msg: to_json_binary(&transfer_voucher_msg)?,
            funds: vec![],
        };

        // Should reject full execution if failed
        response = response
            .add_message(transfer_voucher_msg)
            .add_attribute("partner_fee_transfer", "true")
            .add_attribute(
                "partner_fee_recipient",
                msg.partner_fee_recipient.to_sender_string(),
            )
            .add_attribute("partner_fee_amount", msg.partner_fee_amount.to_string());
    }
    //     let liquidity_response: GetLiquidityResponse = deps.querier.query(
    //         &cosmwasm_std::QueryRequest::Wasm(cosmwasm_std::WasmQuery::Smart {
    //             contract_addr: first_swap.vlp_address.clone(),
    //             msg: to_json_binary(&euclid::msgs::stable_vlp::QueryMsg::Liquidity {})?,
    //         }),
    //     )?;
    //    let swap_msg =  if liquidity_response.token_1_reserve == liquidity_response.token_2_reserve {
    //         return Err(ContractError::Generic {
    //             err: "Liquidity is not enough".to_string(),
    //         });
    //     } else {

    //     }

    let swap_msg = msgs::vlp::ExecuteMsg::Swap {
        sender: sender.clone(),
        asset_in: msg.asset_in.token.clone(),
        amount_in: msg.amount_in,
        min_token_out: msg.min_amount_out,
        next_swaps: next_swaps.to_vec(),
        tx_id: msg.tx_id.clone(),
        test_fail: first_swap.test_fail,
    };

    let msg = WasmMsg::Execute {
        contract_addr: first_swap.vlp_address.clone(),
        msg: to_json_binary(&swap_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(msg, SWAP_REPLY_ID)))
}

fn ibc_execute_deposit_token(
    deps: DepsMut,
    _env: Env,
    msg: ChainIbcDepositTokenExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = msg.clone().sender;

    // Add token 1 in escrow balance
    let token_escrow_key = (msg.asset_in.token.clone(), sender.chain_uid.clone());
    let token_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    let new_escrow_balance = token_escrow_balance.checked_add(msg.amount_in)?;

    ESCROW_BALANCES.save(deps.storage, token_escrow_key, &new_escrow_balance)?;

    let deposit_token_response = DepositTokenResponse {
        amount: msg.amount_in,
        token: msg.asset_in.token.clone(),
        sender: msg.sender.clone(),
        recipient: msg.recipient.clone(),
    };
    let ack = AcknowledgementMsg::Ok(deposit_token_response.clone());

    // Load state to get virtual balance address
    let virtual_balance_address = STATE
        .load(deps.storage)?
        .virtual_balance_address
        .map_or_else(|| Err(ContractError::EmptyVirtualBalanceAddress {}), Ok)?;

    // Send mint msg to virtual balance
    let mint_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: virtual_balance_address.into_string(),
        msg: to_json_binary(&VirtualBalanceMsg::Mint(ExecuteMint {
            amount: msg.amount_in,
            balance_key: BalanceKey {
                cross_chain_user: msg.recipient,
                token_id: msg.asset_in.token.to_string(),
            },
        }))?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_submessage(SubMsg::new(mint_msg))
        .add_attribute("action", "reply_deposit_token")
        .add_attribute(
            "deposit_token_response",
            format!("{deposit_token_response:?}"),
        )
        .add_event(
            tx_event(
                &msg.tx_id,
                &msg.sender.to_sender_string(),
                TxType::DepositToken,
            )
            .add_attribute("tx_id", msg.tx_id.clone()),
        )
        .add_attribute("chain_uid", sender.chain_uid.to_string())
        .add_attribute(
            format!(
                "escrow_added_token_{token}_denom_{denom}",
                token = msg.asset_in.token,
                denom = msg.asset_in.token_type.get_key()
            ),
            msg.amount_in,
        )
        .add_attribute(
            format!(
                "escrow_balance_token_{token}_denom_{denom}",
                token = msg.asset_in.token,
                denom = msg.asset_in.token_type.get_key()
            ),
            new_escrow_balance,
        )
        .set_data(to_json_binary(&ack)?))
}

fn ibc_execute_transfer_virtual_balance(
    deps: DepsMut,
    _env: Env,
    msg: ChainIbcTransferExecuteMsg,
) -> Result<Response, ContractError> {
    let virtual_balance_address = STATE
        .load(deps.storage)?
        .virtual_balance_address
        .ok_or(ContractError::EmptyVirtualBalanceAddress {})?
        .into_string();

    let transfer_voucher_msg =
        euclid::msgs::virtual_balance::ExecuteMsg::Transfer(ExecuteTransfer {
            amount: msg.amount,
            token_id: msg.token.to_string(),
            from: msg.clone().sender,
            to: msg.recipient_address,
        });

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address,
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };

    Ok(Response::default()
        .add_message(transfer_voucher_msg)
        .add_attribute("action", "transfer_virtual_balance")
        .add_event(
            tx_event(
                &msg.tx_id,
                &msg.sender.to_sender_string(),
                TxType::TransferVirtualBalance,
            )
            .add_attribute("tx_id", msg.tx_id.clone()),
        )
        .set_data(to_json_binary(&AcknowledgementMsg::Ok(TransferResponse {
            token: msg.token,
            tx_id: msg.tx_id,
        }))?))
}
