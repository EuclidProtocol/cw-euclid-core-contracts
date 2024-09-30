#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcPacketReceiveMsg,
    IbcReceiveResponse, MessageInfo, Order, Response, StdError, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    deposit::DepositTokenResponse,
    error::ContractError,
    events::{tx_event, TxType},
    fee::Fee,
    msgs::virtual_balance::ExecuteMsg as VirtualBalanceMsg,
    msgs::{self, router::ExecuteMsg, virtual_balance::ExecuteMint},
    pool::EscrowCreationResponse,
    swap::{TransferResponse, WithdrawResponse},
    token::{Pair, Token},
    virtual_balance::BalanceKey,
};
use euclid_ibc::{
    ack::{make_ack_fail, AcknowledgementMsg},
    msg::{
        ChainIbcDepositTokenExecuteMsg, ChainIbcExecuteMsg, ChainIbcRemoveLiquidityExecuteMsg,
        ChainIbcSwapExecuteMsg,
    },
};

use crate::{
    query::validate_swap_pairs,
    reply::{
        ADD_LIQUIDITY_REPLY_ID, IBC_RECEIVE_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID,
        VIRTUAL_BALANCE_MINT_REPLY_ID, VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{
        CHAIN_UID_TO_CHAIN, CHANNEL_TO_CHAIN_UID, DEREGISTERED_CHAINS, ESCROW_BALANCES,
        PENDING_REMOVE_LIQUIDITY, STATE, SWAP_ID_TO_MSG, VLPS,
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

    Ok(IbcReceiveResponse::new()
        .add_attribute("method", "ibc_packet_receive")
        .add_attribute("tx_id", tx_id)
        .set_ack(make_ack_fail("deafult_fail".to_string())?)
        .add_submessage(sub_msg))
}

pub fn ibc_receive_internal_call(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    msg: IbcPacketReceiveMsg,
) -> Result<Response, ContractError> {
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
    let deregistered_chains = DEREGISTERED_CHAINS.load(deps.storage)?;
    ensure!(
        !deregistered_chains.contains(&chain_uid),
        ContractError::DeregisteredChain {}
    );
    let locked = STATE.load(deps.storage)?.locked;
    ensure!(!locked, ContractError::ContractLocked {});
    match msg {
        ChainIbcExecuteMsg::RequestPoolCreation {
            pair,
            sender,
            tx_id,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            execute_request_pool_creation(deps.branch(), env, sender, pair, tx_id)
        }
        ChainIbcExecuteMsg::RequestEscrowCreation {
            token,
            sender,
            tx_id,
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            execute_request_escrow_creation(deps.branch(), env, sender, token, tx_id)
        }
        ChainIbcExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            pair,
            tx_id,
            sender,
            ..
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_add_liquidity(
                deps.branch(),
                env,
                sender,
                token_1_liquidity,
                token_2_liquidity,
                slippage_tolerance,
                pair,
                tx_id,
            )
        }
        ChainIbcExecuteMsg::RemoveLiquidity(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_remove_liquidity(deps.branch(), env, msg)
        }
        ChainIbcExecuteMsg::Swap(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_swap(deps.branch(), env, msg)
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

            Ok(Response::new()
                .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&release_msg)?,
                    funds: vec![],
                }))
                .set_data(to_json_binary(&AcknowledgementMsg::Ok(WithdrawResponse {
                    token: msg.token,
                    tx_id: msg.tx_id,
                }))?))
        }
        ChainIbcExecuteMsg::Transfer(msg) => {
            let release_msg = ExecuteMsg::ReleaseEscrowInternal {
                sender: msg.sender,
                token: msg.token.clone(),
                amount: Some(msg.amount),
                cross_chain_addresses: msg.recipient_addresses,
                timeout: msg.timeout,
                tx_id: msg.tx_id.clone(),
            };

            Ok(Response::new()
                .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&release_msg)?,
                    funds: vec![],
                }))
                .set_data(to_json_binary(&AcknowledgementMsg::Ok(TransferResponse {
                    token: msg.token,
                    tx_id: msg.tx_id,
                }))?))
        }
        ChainIbcExecuteMsg::DepositToken(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );

            ibc_execute_deposit_token(deps.branch(), env, msg)
        }
    }
}

fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    sender: CrossChainUser,
    pair: Pair,
    tx_id: String,
) -> Result<Response, ContractError> {
    pair.validate()?;
    let state = STATE.load(deps.storage)?;

    let vlp = VLPS.may_load(deps.storage, pair.get_tupple())?;

    let register_msg = msgs::vlp::ExecuteMsg::RegisterPool {
        sender: sender.clone(),
        pair: pair.clone(),
        tx_id: tx_id.clone(),
    };

    let response = Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::PoolCreation,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_pool_creation");

    for token in pair.get_vec_token() {
        let token_exists =
            ESCROW_BALANCES.has(deps.storage, (token.clone(), sender.clone().chain_uid));
        let range =
            ESCROW_BALANCES
                .prefix(token)
                .keys_raw(deps.storage, None, None, Order::Ascending);
        // There are two cases
        // token already exists on the sender chain - We can safely assume that this was validated already by factory so allow pool creation
        // token not present in sender chain -  This token should not have escrow on any other chain, i.e. This should be completely new token
        ensure!(
            token_exists || range.take(1).count() == 0,
            ContractError::new("Cannot use already existing token without registering it first")
        )
    }

    // If vlp is already there, send execute msg to it to register the pool, else create a new pool with register msg attached to instantiate msg
    if vlp.is_some() {
        let msg = WasmMsg::Execute {
            contract_addr: vlp.unwrap(),
            msg: to_json_binary(&register_msg)?,
            funds: vec![],
        };
        Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_POOL_REGISTER_REPLY_ID)))
    } else {
        let instantiate_msg = msgs::vlp::InstantiateMsg {
            router: env.contract.address.to_string(),
            virtual_balance: state
                .virtual_balance_address
                .ok_or(ContractError::Generic {
                    err: "virtual balance not instantiated".to_string(),
                })?
                .to_string(),
            pair,
            fee: Fee {
                lp_fee_bps: 10,
                euclid_fee_bps: 10,
                recipient: CrossChainUser {
                    address: state.admin.clone(),
                    chain_uid: ChainUid::vsl_chain_uid()?,
                },
            },
            execute: Some(register_msg),
            admin: state.admin.clone(),
        };
        let msg = WasmMsg::Instantiate {
            admin: Some(state.admin),
            code_id: state.vlp_code_id,
            msg: to_json_binary(&instantiate_msg)?,
            funds: vec![],
            label: "VLP".to_string(),
        };
        Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID)))
    }
}

fn execute_request_escrow_creation(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: Token,
    tx_id: String,
) -> Result<Response, ContractError> {
    token.validate()?;

    let token_exists = ESCROW_BALANCES.has(deps.storage, (token.clone(), sender.clone().chain_uid));
    ensure!(!token_exists, ContractError::TokenAlreadyExist {});

    ESCROW_BALANCES.save(
        deps.storage,
        (token.clone(), sender.clone().chain_uid),
        &Uint128::zero(),
    )?;

    let ack = AcknowledgementMsg::Ok(EscrowCreationResponse {});
    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::EscrowCreation,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_escrow_creation")
        .set_data(to_json_binary(&ack)?))
}

fn ibc_execute_add_liquidity(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    pair: Pair,
    tx_id: String,
) -> Result<Response, ContractError> {
    let vlp_address = VLPS.load(deps.storage, pair.get_tupple())?;
    let pool_liquidity: euclid::msgs::vlp::GetLiquidityResponse = deps.querier.query_wasm_smart(
        vlp_address.clone(),
        &euclid::msgs::vlp::QueryMsg::Liquidity {},
    )?;

    let mut response = Response::new().add_event(
        tx_event(&tx_id, &sender.to_sender_string(), TxType::AddLiquidity)
            .add_attribute("tx_id", tx_id.clone()),
    );
    // Increase token 1 escrow balance
    let token_1_escrow_key = (
        pool_liquidity.pair.token_1.clone(),
        sender.chain_uid.clone(),
    );
    let token_1_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_1_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    ESCROW_BALANCES.save(
        deps.storage,
        token_1_escrow_key,
        &token_1_escrow_balance.checked_add(token_1_liquidity)?,
    )?;

    // Increase token 2 escrow balance
    let token_2_escrow_key = (
        pool_liquidity.pair.token_2.clone(),
        sender.chain_uid.clone(),
    );
    let token_2_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_2_escrow_key.clone())?
        .unwrap_or(Uint128::zero());
    ESCROW_BALANCES.save(
        deps.storage,
        token_2_escrow_key,
        &token_2_escrow_balance.checked_add(token_2_liquidity)?,
    )?;

    let virtual_balance_address =
        STATE
            .load(deps.storage)?
            .virtual_balance_address
            .ok_or(ContractError::Generic {
                err: "virtual balance address doesn't exist".to_string(),
            })?;

    let mint_virtual_balance_msg = euclid::msgs::virtual_balance::ExecuteMsg::Mint(ExecuteMint {
        amount: token_1_liquidity,
        balance_key: BalanceKey {
            cross_chain_user: CrossChainUser {
                address: vlp_address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            token_id: pool_liquidity.pair.token_1.to_string(),
        },
    });

    let mint_virtual_balance_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&mint_virtual_balance_msg)?,
        funds: vec![],
    };

    response = response.add_submessage(SubMsg::reply_on_error(
        mint_virtual_balance_msg,
        VIRTUAL_BALANCE_MINT_REPLY_ID,
    ));

    let mint_virtual_balance_msg = euclid::msgs::virtual_balance::ExecuteMsg::Mint(ExecuteMint {
        amount: token_2_liquidity,
        balance_key: BalanceKey {
            cross_chain_user: CrossChainUser {
                address: vlp_address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            token_id: pool_liquidity.pair.token_2.to_string(),
        },
    });

    let mint_virtual_balance_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&mint_virtual_balance_msg)?,
        funds: vec![],
    };

    response = response.add_submessage(SubMsg::reply_on_error(
        mint_virtual_balance_msg,
        VIRTUAL_BALANCE_MINT_REPLY_ID,
    ));

    let add_liquidity_msg = msgs::vlp::ExecuteMsg::AddLiquidity {
        token_1_liquidity,
        token_2_liquidity,
        slippage_tolerance,
        sender,
        tx_id,
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
        first_swap.token_in == msg.asset_in,
        ContractError::new("Asset IN doen't match router")
    );

    ensure!(
        last_swap.token_out == msg.asset_out,
        ContractError::new("Asset OUT doen't match router")
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

    // Add token 1 in escrow balance
    let token_escrow_key = (msg.asset_in.clone(), sender.chain_uid.clone());
    let token_1_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    ESCROW_BALANCES.save(
        deps.storage,
        token_escrow_key,
        &token_1_escrow_balance.checked_add(msg.amount_in)?,
    )?;

    // Mint virtual balance for the first swap vlp so it can start processing tx
    let mint_virtual_balance_msg = euclid::msgs::virtual_balance::ExecuteMsg::Mint(ExecuteMint {
        amount: msg.amount_in,
        balance_key: BalanceKey {
            cross_chain_user: CrossChainUser {
                address: first_swap.vlp_address.clone(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            token_id: msg.asset_in.to_string(),
        },
    });

    let mint_virtual_balance_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.to_string(),
        msg: to_json_binary(&mint_virtual_balance_msg)?,
        funds: vec![],
    };

    response = response.add_submessage(SubMsg::reply_always(
        mint_virtual_balance_msg,
        VIRTUAL_BALANCE_MINT_REPLY_ID,
    ));

    let swap_msg = msgs::vlp::ExecuteMsg::Swap {
        sender: sender.clone(),
        asset_in: msg.asset_in.clone(),
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
    let token_escrow_key = (msg.asset_in.clone(), sender.chain_uid.clone());
    let token_1_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    ESCROW_BALANCES.save(
        deps.storage,
        token_escrow_key,
        &token_1_escrow_balance.checked_add(msg.amount_in)?,
    )?;

    let deposit_token_response = DepositTokenResponse {
        amount: msg.amount_in,
        token: msg.asset_in.clone(),
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
                token_id: msg.asset_in.to_string(),
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
        .set_data(to_json_binary(&ack)?))
}
