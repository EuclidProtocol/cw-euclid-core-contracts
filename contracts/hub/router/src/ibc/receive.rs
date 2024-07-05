#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcPacketReceiveMsg,
    IbcReceiveResponse, Response, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    events::{tx_event, TxType},
    fee::Fee,
    msgs::{self, router::ExecuteMsg, vcoin::ExecuteMint},
    token::Pair,
    vcoin::BalanceKey,
};
use euclid_ibc::{
    ack::make_ack_fail,
    msg::{ChainIbcExecuteMsg, ChainIbcRemoveLiquidityExecuteMsg, ChainIbcSwapExecuteMsg},
};

use crate::{
    query::validate_swap_pairs,
    reply::{
        ADD_LIQUIDITY_REPLY_ID, IBC_RECEIVE_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID,
        VCOIN_MINT_REPLY_ID, VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{
        CHAIN_UID_TO_CHAIN, CHANNEL_TO_CHAIN_UID, ESCROW_BALANCES, PENDING_REMOVE_LIQUIDITY, STATE,
        SWAP_ID_TO_MSG, VLPS,
    },
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
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
    Ok(IbcReceiveResponse::new()
        .add_submessage(sub_msg)
        .add_attribute("ibc_ack", format!("{msg:?}"))
        .add_attribute("method", "ibc_packet_receive")
        .set_ack(make_ack_fail("deafult_fail".to_string())?))
}

pub fn ibc_receive_internal_call(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<Response, ContractError> {
    // Get the chain data from current channel received
    let channel = msg.packet.dest.channel_id;
    let chain_uid = CHANNEL_TO_CHAIN_UID.load(deps.storage, channel)?;
    let _chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
    let msg: ChainIbcExecuteMsg = from_json(msg.packet.data)?;
    match msg {
        ChainIbcExecuteMsg::RequestPoolCreation {
            pair,
            sender,
            tx_id,
            ..
        } => {
            ensure!(
                sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            execute_request_pool_creation(deps, env, sender, pair, tx_id)
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
                deps,
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
            ibc_execute_remove_liquidity(deps, env, msg)
        }
        ChainIbcExecuteMsg::Swap(msg) => {
            ensure!(
                msg.sender.chain_uid == chain_uid,
                ContractError::new("Chain UID mismatch")
            );
            ibc_execute_swap(deps, env, msg)
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
        .add_attribute("method", "request_pool_creation");
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
            vcoin: state
                .vcoin_address
                .ok_or(ContractError::Generic {
                    err: "vcoin not instantiated".to_string(),
                })?
                .to_string(),
            pair,
            fee: Fee {
                lp_fee: 0,
                treasury_fee: 0,
                staker_fee: 0,
            },
            execute: Some(register_msg),
        };
        let msg = WasmMsg::Instantiate {
            admin: Some(env.contract.address.to_string()),
            code_id: state.vlp_code_id,
            msg: to_json_binary(&instantiate_msg)?,
            funds: vec![],
            label: "VLP".to_string(),
        };
        Ok(response.add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID)))
    }
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
        &euclid::msgs::vlp::QueryMsg::Liquidity { height: None },
    )?;

    let mut response = Response::new().add_event(tx_event(
        &tx_id,
        &sender.to_sender_string(),
        TxType::AddLiquidity,
    ));
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

    let vcoin_address = STATE
        .load(deps.storage)?
        .vcoin_address
        .ok_or(ContractError::Generic {
            err: "vcoin address doesn't exist".to_string(),
        })?;

    let mint_vcoin_msg = euclid::msgs::vcoin::ExecuteMsg::Mint(ExecuteMint {
        amount: token_1_liquidity,
        balance_key: BalanceKey {
            cross_chain_user: CrossChainUser {
                address: vlp_address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            token_id: pool_liquidity.pair.token_1.to_string(),
        },
    });

    let mint_vcoin_msg = WasmMsg::Execute {
        contract_addr: vcoin_address.to_string(),
        msg: to_json_binary(&mint_vcoin_msg)?,
        funds: vec![],
    };

    response = response.add_submessage(SubMsg::reply_on_error(mint_vcoin_msg, VCOIN_MINT_REPLY_ID));

    let mint_vcoin_msg = euclid::msgs::vcoin::ExecuteMsg::Mint(ExecuteMint {
        amount: token_2_liquidity,
        balance_key: BalanceKey {
            cross_chain_user: CrossChainUser {
                address: vlp_address.to_string(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            token_id: pool_liquidity.pair.token_2.to_string(),
        },
    });

    let mint_vcoin_msg = WasmMsg::Execute {
        contract_addr: vcoin_address.to_string(),
        msg: to_json_binary(&mint_vcoin_msg)?,
        funds: vec![],
    };

    response = response.add_submessage(SubMsg::reply_on_error(mint_vcoin_msg, VCOIN_MINT_REPLY_ID));

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
    let response = Response::new().add_event(tx_event(
        &msg.tx_id,
        &msg.sender.to_sender_string(),
        TxType::AddLiquidity,
    ));

    let req_key = (
        msg.sender.chain_uid.clone(),
        msg.sender.address.clone(),
        msg.tx_id.clone(),
    );
    ensure!(
        !PENDING_REMOVE_LIQUIDITY.has(deps.storage, req_key.clone()),
        ContractError::new("tx already present")
    );

    PENDING_REMOVE_LIQUIDITY.save(deps.storage, req_key, &msg)?;

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

    SWAP_ID_TO_MSG.save(deps.storage, req_key, &msg);

    let mut response = Response::new().add_event(tx_event(
        &msg.tx_id,
        &msg.sender.to_sender_string(),
        TxType::Swap,
    ));

    let sender = msg.sender;

    let vcoin_address = STATE
        .load(deps.storage)?
        .vcoin_address
        .ok_or(ContractError::Generic {
            err: "vcoin address doesn't exist".to_string(),
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

    // Mint vcoin for the first swap vlp so it can start processing tx
    let mint_vcoin_msg = euclid::msgs::vcoin::ExecuteMsg::Mint(ExecuteMint {
        amount: msg.amount_in,
        balance_key: BalanceKey {
            cross_chain_user: CrossChainUser {
                address: first_swap.vlp_address.clone(),
                chain_uid: ChainUid::vsl_chain_uid()?,
            },
            token_id: msg.asset_in.to_string(),
        },
    });

    let mint_vcoin_msg = WasmMsg::Execute {
        contract_addr: vcoin_address.to_string(),
        msg: to_json_binary(&mint_vcoin_msg)?,
        funds: vec![],
    };

    response = response.add_submessage(SubMsg::reply_always(mint_vcoin_msg, VCOIN_MINT_REPLY_ID));

    let swap_msg = msgs::vlp::ExecuteMsg::Swap {
        sender: sender.clone(),
        asset_in: msg.asset_in.clone(),
        amount_in: msg.amount_in.clone(),
        min_token_out: msg.min_amount_out.clone(),
        next_swaps: next_swaps.to_vec(),
        tx_id: msg.tx_id.clone(),
    };

    let msg = WasmMsg::Execute {
        contract_addr: first_swap.vlp_address.clone(),
        msg: to_json_binary(&swap_msg)?,
        funds: vec![],
    };
    Ok(response.add_submessage(SubMsg::reply_always(msg, SWAP_REPLY_ID)))
}
