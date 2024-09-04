use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, Uint128};
use cw2::set_contract_version;
use euclid::fee::{DenomFees, TotalFees};

use crate::reply::{NEXT_SWAP_REPLY_ID, VIRTUAL_BALANCE_TRANSFER_REPLY_ID};
use crate::state::{State, BALANCES, STATE};
use crate::{execute, reply};
use euclid::error::ContractError;
use euclid::msgs::vlp::{ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::query::{
    query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap, query_state,
    query_total_fees_collected, query_total_fees_per_denom,
};
// version info for migration info
const CONTRACT_NAME: &str = "crates.io:vlp";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Validate token pair
    msg.pair.validate()?;

    let state = State {
        pair: msg.pair,
        virtual_balance: msg.virtual_balance,
        router: info.sender.to_string(),
        fee: msg.fee,
        total_fees_collected: TotalFees {
            lp_fees: DenomFees {
                totals: HashMap::default(),
            },
            euclid_fees: DenomFees {
                totals: HashMap::default(),
            },
        },
        last_updated: 0,
        total_lp_tokens: Uint128::zero(),
        admin: msg.admin,
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    BALANCES.save(deps.storage, state.pair.token_1, &Uint128::zero())?;
    BALANCES.save(deps.storage, state.pair.token_2, &Uint128::zero())?;

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool {
                    sender,
                    pair,
                    tx_id,
                } => execute::register_pool(deps, env.clone(), info.clone(), sender, pair, tx_id),
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool {
            sender,
            pair,
            tx_id,
        } => execute::register_pool(deps, env, info, sender, pair, tx_id),
        ExecuteMsg::UpdateFee {
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        } => execute::update_fee(deps, info, lp_fee_bps, euclid_fee_bps, recipient),
        ExecuteMsg::AddLiquidity {
            sender,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            tx_id,
        } => execute::add_liquidity(
            deps,
            env,
            info,
            sender,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            tx_id,
        ),
        ExecuteMsg::RemoveLiquidity {
            sender,
            lp_allocation,
            tx_id,
        } => execute::remove_liquidity(deps, env, info, sender, lp_allocation, tx_id),
        ExecuteMsg::Swap {
            sender,
            asset_in,
            amount_in,
            min_token_out,
            tx_id,
            next_swaps,
            test_fail,
        } => execute::execute_swap(
            deps,
            env,
            sender,
            asset_in,
            amount_in,
            min_token_out,
            tx_id,
            next_swaps,
            test_fail,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::SimulateSwap {
            asset,
            asset_amount,
            swaps,
        } => query_simulate_swap(deps, asset, asset_amount, swaps),
        QueryMsg::Liquidity {} => query_liquidity(deps, env),
        QueryMsg::Fee {} => query_fee(deps),
        QueryMsg::TotalFeesCollected {} => query_total_fees_collected(deps),
        QueryMsg::TotalFeesPerDenom { denom } => query_total_fees_per_denom(deps, denom),
        QueryMsg::Pool { chain_uid } => query_pool(deps, chain_uid),

        QueryMsg::GetAllPools {} => query_all_pools(deps),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        VIRTUAL_BALANCE_TRANSFER_REPLY_ID => reply::on_virtual_balance_transfer_reply(deps, msg),
        NEXT_SWAP_REPLY_ID => reply::on_next_swap_reply(deps, msg),

        id => Err(ContractError::Generic {
            err: format!("Unknown reply id: {id}"),
        }),
    }
}
