#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, DepsMut, Env, IbcPacketReceiveMsg, IbcReceiveResponse,
    SubMsg, Uint128, WasmMsg,
};
use euclid::{
    error::ContractError,
    fee::Fee,
    msgs::{self, vcoin::ExecuteMint},
    token::PairInfo,
    vcoin::BalanceKey,
};
use euclid_ibc::{
    ack::make_ack_fail,
    msg::{ChainIbcExecuteMsg, ChainIbcSwapExecuteMsg},
};

use crate::{
    query::validate_swap_vlps,
    reply::{
        ADD_LIQUIDITY_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID, VCOIN_MINT_REPLY_ID,
        VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{CHAIN_ID_TO_CHAIN, CHANNEL_TO_CHAIN_ID, ESCROW_BALANCES, STATE, SWAP_ID_TO_MSG, VLPS},
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // Regardless of if our processing of this packet works we need to
    // commit an ACK to the chain. As such, we wrap all handling logic
    // in a seprate function and on error write out an error ack.
    match do_ibc_packet_receive(deps, env, msg) {
        Ok(response) => Ok(response),
        Err(error) => Ok(IbcReceiveResponse::new()
            .add_attribute("method", "ibc_packet_receive")
            .add_attribute("error", error.to_string())
            .set_ack(make_ack_fail(error.to_string())?)),
    }
}

pub fn do_ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // Get the chain data from current channel received
    let channel = msg.packet.dest.channel_id;
    let chain_id = CHANNEL_TO_CHAIN_ID.load(deps.storage, channel)?;
    let chain = CHAIN_ID_TO_CHAIN.load(deps.storage, chain_id)?;
    let msg: ChainIbcExecuteMsg = from_json(msg.packet.data)?;
    match msg {
        ChainIbcExecuteMsg::RequestPoolCreation { pair_info, .. } => {
            execute_request_pool_creation(deps, env, chain.factory_chain_id, pair_info)
        }
        ChainIbcExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            vlp_address,
            outpost_sender,
            ..
        } => ibc_execute_add_liquidity(
            deps,
            env,
            chain.factory_chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            vlp_address,
            outpost_sender,
        ),
        ChainIbcExecuteMsg::RemoveLiquidity {
            chain_id,
            vlp_address,
            lp_allocation,
            outpost_sender,
        } => ibc_execute_remove_liquidity(
            deps,
            env,
            chain_id,
            lp_allocation,
            vlp_address,
            outpost_sender,
        ),
        ChainIbcExecuteMsg::Swap(msg) => ibc_execute_swap(deps, env, chain.factory_chain_id, msg),
        _ => Err(ContractError::NotImplemented {}),
    }
}

fn execute_request_pool_creation(
    deps: DepsMut,
    env: Env,
    chain_id: String,
    pair_info: PairInfo,
) -> Result<IbcReceiveResponse, ContractError> {
    let state = STATE.load(deps.storage)?;

    let pair = (pair_info.token_1.get_token(), pair_info.token_2.get_token());

    let vlp = VLPS.may_load(deps.storage, pair)?;

    // Check if VLP exist for pair, in correct order. We don't want to create new VLP if just token_1 and 2 are reversed
    if vlp.is_some() {
        let pair = (pair_info.token_2.get_token(), pair_info.token_1.get_token());
        ensure!(
            VLPS.load(deps.storage, pair).is_err(),
            ContractError::Generic {
                err: "pair order is reversed".to_string()
            }
        );
    }

    let register_msg = msgs::vlp::ExecuteMsg::RegisterPool {
        chain_id,
        pair_info: pair_info.clone(),
    };

    // If vlp is already there, send execute msg to it to register the pool, else create a new pool with register msg attached to instantiate msg
    if vlp.is_some() {
        let msg = WasmMsg::Execute {
            contract_addr: vlp.unwrap(),
            msg: to_json_binary(&register_msg)?,
            funds: vec![],
        };
        Ok(IbcReceiveResponse::new()
            .add_submessage(SubMsg::reply_always(msg, VLP_POOL_REGISTER_REPLY_ID))
            .set_ack(make_ack_fail("Reply Failed".to_string())?))
    } else {
        let instantiate_msg = msgs::vlp::InstantiateMsg {
            router: env.contract.address.to_string(),
            vcoin: state
                .vcoin_address
                .ok_or(ContractError::Generic {
                    err: "vcoin not instantiated".to_string(),
                })?
                .to_string(),
            cw20_code_id: state.cw20_code_id,
            pair: pair_info.get_pair(),
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
        Ok(IbcReceiveResponse::new()
            .add_submessage(SubMsg::reply_always(msg, VLP_INSTANTIATE_REPLY_ID))
            .set_ack(make_ack_fail("Reply Failed".to_string())?))
    }
}

fn ibc_execute_add_liquidity(
    deps: DepsMut,
    env: Env,
    chain_id: String,
    token_1_liquidity: Uint128,
    token_2_liquidity: Uint128,
    slippage_tolerance: u64,
    vlp_address: String,
    outpost_sender: String,
) -> Result<IbcReceiveResponse, ContractError> {
    let pair: euclid::msgs::vlp::GetLiquidityResponse = deps.querier.query_wasm_smart(
        vlp_address.clone(),
        &euclid::msgs::vlp::QueryMsg::Liquidity {},
    )?;

    let add_liquidity_msg = msgs::vlp::ExecuteMsg::AddLiquidity {
        chain_id: chain_id.clone(),
        token_1_liquidity,
        token_2_liquidity,
        slippage_tolerance,
        outpost_sender,
    };

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address.clone(),
        msg: to_json_binary(&add_liquidity_msg)?,
        funds: vec![],
    };

    let vcoin_address = STATE
        .load(deps.storage)?
        .vcoin_address
        .ok_or(ContractError::Generic {
            err: "vcoin address doesn't exist".to_string(),
        })?;

    let mint_vcoin_msg = euclid::msgs::vcoin::ExecuteMsg::Mint(ExecuteMint {
        amount: token_1_liquidity,
        balance_key: BalanceKey {
            chain_id: env.block.chain_id.clone(),
            address: vlp_address.to_string(),
            token_id: pair.pair.token_1.id.clone(),
        },
    });

    let mint_vcoin_msg = WasmMsg::Execute {
        contract_addr: vcoin_address.to_string(),
        msg: to_json_binary(&mint_vcoin_msg)?,
        funds: vec![],
    };

    let mint_vcoin_2_msg = euclid::msgs::vcoin::ExecuteMsg::Mint(ExecuteMint {
        amount: token_2_liquidity,
        balance_key: BalanceKey {
            chain_id: env.block.chain_id,
            address: vlp_address.to_string(),
            token_id: pair.pair.token_2.id.clone(),
        },
    });

    let mint_vcoin_2_msg = WasmMsg::Execute {
        contract_addr: vcoin_address.to_string(),
        msg: to_json_binary(&mint_vcoin_2_msg)?,
        funds: vec![],
    };

    let token_1_escrow_key = (pair.pair.token_1, chain_id.clone());
    let token_1_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_1_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    ESCROW_BALANCES.save(
        deps.storage,
        token_1_escrow_key,
        &token_1_escrow_balance.checked_add(token_1_liquidity)?,
    )?;

    let token_2_escrow_key = (pair.pair.token_2, chain_id.clone());
    let token_2_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_2_escrow_key.clone())?
        .unwrap_or(Uint128::zero());
    ESCROW_BALANCES.save(
        deps.storage,
        token_2_escrow_key,
        &token_2_escrow_balance.checked_add(token_2_liquidity)?,
    )?;

    Ok(IbcReceiveResponse::new()
        .add_submessage(SubMsg::reply_always(mint_vcoin_msg, VCOIN_MINT_REPLY_ID))
        .add_submessage(SubMsg::reply_always(mint_vcoin_2_msg, VCOIN_MINT_REPLY_ID))
        .add_submessage(SubMsg::reply_always(msg, ADD_LIQUIDITY_REPLY_ID))
        .set_ack(make_ack_fail("Reply Failed".to_string())?))
}

fn ibc_execute_remove_liquidity(
    _deps: DepsMut,
    _env: Env,
    chain_id: String,
    lp_allocation: Uint128,
    vlp_address: String,
    outpost_sender: String,
) -> Result<IbcReceiveResponse, ContractError> {
    let remove_liquidity_msg = msgs::vlp::ExecuteMsg::RemoveLiquidity {
        chain_id,
        lp_allocation,
        outpost_sender,
    };

    let msg = WasmMsg::Execute {
        contract_addr: vlp_address,
        msg: to_json_binary(&remove_liquidity_msg)?,
        funds: vec![],
    };
    Ok(IbcReceiveResponse::new()
        .add_submessage(SubMsg::reply_always(msg, REMOVE_LIQUIDITY_REPLY_ID))
        .set_ack(make_ack_fail("Reply Failed".to_string())?))
}

fn ibc_execute_swap(
    deps: DepsMut,
    env: Env,
    factory_chain: String,
    msg: ChainIbcSwapExecuteMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    ensure!(
        validate_swap_vlps(deps.as_ref(), &msg.swaps).is_ok(),
        ContractError::Generic {
            err: "VLPS listed in swaps are not registered".to_string()
        }
    );

    let (first_swap, next_swaps) = msg.swaps.split_first().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;
    let vcoin_address = STATE
        .load(deps.storage)?
        .vcoin_address
        .ok_or(ContractError::Generic {
            err: "vcoin address doesn't exist".to_string(),
        })?;

    let token_escrow_key = (msg.asset_in.clone(), factory_chain.clone());
    let token_1_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    ESCROW_BALANCES.save(
        deps.storage,
        token_escrow_key,
        &token_1_escrow_balance.checked_add(msg.amount_in)?,
    )?;

    let mint_vcoin_msg = euclid::msgs::vcoin::ExecuteMsg::Mint(ExecuteMint {
        amount: msg.amount_in,
        balance_key: BalanceKey {
            chain_id: env.block.chain_id.clone(),
            address: first_swap.vlp_address.to_string(),
            token_id: msg.asset_in.id.clone(),
        },
    });

    let mint_vcoin_msg = WasmMsg::Execute {
        contract_addr: vcoin_address.to_string(),
        msg: to_json_binary(&mint_vcoin_msg)?,
        funds: vec![],
    };

    let swap_msg = msgs::vlp::ExecuteMsg::Swap {
        to_address: msg.to_address.clone(),
        to_chain_id: msg.to_chain_id.clone(),
        asset_in: msg.asset_in.clone(),
        amount_in: msg.amount_in,
        min_token_out: msg.min_amount_out,
        swap_id: msg.swap_id.clone(),
        next_swaps: next_swaps.to_vec(),
    };

    SWAP_ID_TO_MSG.save(deps.storage, msg.swap_id.clone(), &msg)?;

    let msg = WasmMsg::Execute {
        contract_addr: first_swap.vlp_address.clone(),
        msg: to_json_binary(&swap_msg)?,
        funds: vec![],
    };
    Ok(IbcReceiveResponse::new()
        .add_submessage(SubMsg::reply_always(mint_vcoin_msg, VCOIN_MINT_REPLY_ID))
        .add_submessage(SubMsg::reply_always(msg, SWAP_REPLY_ID))
        .set_ack(make_ack_fail("Reply Failed".to_string())?))
}
