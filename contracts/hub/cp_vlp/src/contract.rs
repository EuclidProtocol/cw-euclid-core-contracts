use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::{
    entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, Uint128,
};
use cw2::set_contract_version;
use euclid::{
    error::ContractError,
    fee::{DenomFees, TotalFees},
    msgs::vlp::{
        base::{State, NEXT_SWAP_REPLY_ID},
        cp::msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
    },
};
use euclid_pool::{
    add_liquidity, execute_swap, register_pool, remove_liquidity, update_admin, update_fee,
    SwapCalculationMethod,
};

use crate::{
    query::{
        query_admin, query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap,
        query_state, query_total_fees_collected, query_total_fees_per_denom,
    },
    reply,
    state::{ADMIN, BALANCES, CHAIN_LP_TOKENS, COLLATERAL_LP_TOKENS, STATE},
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
        virtual_balance_contract: msg.virtual_balance_contract,
        router: info.sender.clone(),
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
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &msg.admin)?;

    BALANCES.save(deps.storage, state.pair.token_1, &Uint128::zero())?;
    BALANCES.save(deps.storage, state.pair.token_2, &Uint128::zero())?;

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool(register_pool_msg) => register_pool(
                    deps,
                    env.clone(),
                    info.clone(),
                    &STATE,
                    &CHAIN_LP_TOKENS,
                    None,
                    register_pool_msg.sender,
                    register_pool_msg.pair,
                    register_pool_msg.tx_id,
                ),
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.contract.address.to_string())
        .add_attribute("owner", info.sender)
        .add_attribute("pool_type", "xyk"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool(register_pool_msg) => register_pool(
            deps,
            env,
            info,
            &STATE,
            &CHAIN_LP_TOKENS,
            None,
            register_pool_msg.sender,
            register_pool_msg.pair,
            register_pool_msg.tx_id,
        ),
        ExecuteMsg::AddLiquidity(add_liquidity_msg) => add_liquidity(
            deps,
            env,
            info,
            &STATE,
            &BALANCES,
            &CHAIN_LP_TOKENS,
            &COLLATERAL_LP_TOKENS,
            add_liquidity_msg.sender,
            add_liquidity_msg.liquidity,
            add_liquidity_msg.slippage_tolerance_bps,
            add_liquidity_msg.tx_id,
        ),
        ExecuteMsg::RemoveLiquidity(remove_liquidity_msg) => remove_liquidity(
            deps,
            env,
            info,
            &STATE,
            &BALANCES,
            &CHAIN_LP_TOKENS,
            remove_liquidity_msg.sender,
            remove_liquidity_msg.lp_allocation,
            remove_liquidity_msg.tx_id,
        ),
        ExecuteMsg::Swap(swap_msg) => execute_swap(
            deps,
            env,
            info,
            &STATE,
            &BALANCES,
            swap_msg.sender,
            swap_msg.asset_in,
            swap_msg.amount_in,
            swap_msg.min_token_out,
            swap_msg.tx_id,
            swap_msg.next_swaps,
            SwapCalculationMethod::Regular,
            swap_msg.test_fail,
        ),
        ExecuteMsg::UpdateFee {
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        } => update_fee(
            deps,
            info,
            &STATE,
            &ADMIN,
            lp_fee_bps,
            euclid_fee_bps,
            recipient,
        ),
        ExecuteMsg::UpdateAdmin { admin, admin_type } => {
            update_admin(deps, env, info, &ADMIN, admin, admin_type)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::GetAdmin {} => query_admin(deps),
        QueryMsg::SimulateSwap(msg) => {
            query_simulate_swap(deps, msg.asset, msg.asset_amount, msg.swaps)
        }
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
        NEXT_SWAP_REPLY_ID => reply::on_next_swap_reply(deps, msg),

        id => Err(ContractError::Generic {
            err: format!("Unknown reply id: {id}"),
        }),
    }
}
