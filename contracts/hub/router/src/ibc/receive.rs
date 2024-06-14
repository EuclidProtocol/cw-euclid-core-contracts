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
    swap::NextSwap,
    token::{PairInfo, Token},
    vcoin::BalanceKey,
};
use euclid_ibc::{ack::make_ack_fail, msg::ChainIbcExecuteMsg};

use crate::{
    reply::{
        ADD_LIQUIDITY_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID, VCOIN_MINT_REPLY_ID,
        VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
    },
    state::{CHAIN_ID_TO_CHAIN, CHANNEL_TO_CHAIN_ID, ESCROW_BALANCES, STATE, VLPS},
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
            ..
        } => ibc_execute_add_liquidity(
            deps,
            env,
            chain.factory_chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            vlp_address,
        ),
        ChainIbcExecuteMsg::RemoveLiquidity {
            chain_id,
            vlp_address,
            lp_allocation,
        } => ibc_execute_remove_liquidity(deps, env, chain_id, lp_allocation, vlp_address),
        ChainIbcExecuteMsg::Swap {
            asset_in,
            amount_in,
            min_amount_out,
            swap_id,
            swaps,
            to_chain_id,
            to_address,
            ..
        } => ibc_execute_swap(
            deps,
            env,
            chain.factory_chain_id,
            to_chain_id,
            to_address,
            asset_in,
            amount_in,
            min_amount_out,
            swap_id,
            swaps,
        ),
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
) -> Result<IbcReceiveResponse, ContractError> {
    let remove_liquidity_msg = msgs::vlp::ExecuteMsg::RemoveLiquidity {
        chain_id,
        lp_allocation,
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
    to_chain_id: String,
    to_address: String,
    asset_in: Token,
    amount_in: Uint128,
    min_token_out: Uint128,
    swap_id: String,
    swaps: Vec<NextSwap>,
) -> Result<IbcReceiveResponse, ContractError> {
    let all_vlps: Result<Vec<String>, ContractError> = VLPS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let item = item?;
            Ok(item.1)
        })
        .collect();

    let all_vlps = all_vlps?;

    // Do an early check that all vlps are present
    for swap in swaps.clone() {
        ensure!(
            all_vlps.contains(&swap.vlp_address),
            ContractError::UnsupportedOperation {}
        );
    }

    let (first_swap, next_swaps) = swaps.split_first().ok_or(ContractError::Generic {
        err: "Swaps cannot be empty".to_string(),
    })?;

    let token_escrow_key = (asset_in.clone(), factory_chain.clone());
    let token_1_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    ESCROW_BALANCES.save(
        deps.storage,
        token_escrow_key,
        &token_1_escrow_balance.checked_add(amount_in)?,
    )?;

    let vcoin_address = STATE
        .load(deps.storage)?
        .vcoin_address
        .ok_or(ContractError::Generic {
            err: "vcoin address doesn't exist".to_string(),
        })?;

    let mint_vcoin_msg = euclid::msgs::vcoin::ExecuteMsg::Mint(ExecuteMint {
        amount: amount_in,
        balance_key: BalanceKey {
            chain_id: env.block.chain_id.clone(),
            address: first_swap.vlp_address.to_string(),
            token_id: asset_in.id.clone(),
        },
    });

    let mint_vcoin_msg = WasmMsg::Execute {
        contract_addr: vcoin_address.to_string(),
        msg: to_json_binary(&mint_vcoin_msg)?,
        funds: vec![],
    };

    let swap_msg = msgs::vlp::ExecuteMsg::Swap {
        to_address,
        to_chain_id,
        asset_in,
        amount_in,
        min_token_out,
        swap_id,
        next_swaps: next_swaps.to_vec(),
    };

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
