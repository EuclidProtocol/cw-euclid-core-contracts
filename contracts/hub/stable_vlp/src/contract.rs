use std::collections::HashMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, Uint128};
use cw2::set_contract_version;
use euclid::fee::{DenomFees, TotalFees};

use crate::query::{
    query_admin, query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap,
    query_state, query_total_fees_collected, query_total_fees_per_denom,
};
use crate::reply;
use crate::state::{ADMIN, AMP_FACTOR, BALANCES, CHAIN_LP_TOKENS, COLLATERAL_LP_TOKENS, STATE};
use euclid::error::ContractError;
use euclid::msgs::vlp::base::{State, NEXT_SWAP_REPLY_ID};
use euclid::msgs::vlp::stable::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, DEFAULT_AMP_FACTOR};
use euclid_pool::{
    add_liquidity, execute_swap, register_pool, remove_liquidity, update_admin, update_amp_factor,
    update_fee, SwapCalculationMethod,
};
// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:stable_vlp";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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

    let amp_factor = msg.amp_factor.unwrap_or(DEFAULT_AMP_FACTOR);
    AMP_FACTOR.save(deps.storage, &amp_factor)?;

    let response =
        msg.execute
            .map_or(Ok(Response::default()), |execute_msg| match execute_msg {
                ExecuteMsg::RegisterPool(register_pool_msg) => register_pool(
                    deps,
                    env.clone(),
                    info.clone(),
                    &STATE,
                    &CHAIN_LP_TOKENS,
                    Some(amp_factor),
                    register_pool_msg.sender,
                    register_pool_msg.pair,
                    register_pool_msg.tx_id,
                ),
                _ => Err(ContractError::Unauthorized {}),
            })?;

    Ok(response
        .add_attribute("action", "instantiate")
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
        ExecuteMsg::RegisterPool(register_pool_msg) => {
            let amp_factor = AMP_FACTOR.load(deps.storage).unwrap_or(DEFAULT_AMP_FACTOR);
            register_pool(
                deps,
                env,
                info,
                &STATE,
                &CHAIN_LP_TOKENS,
                Some(amp_factor),
                register_pool_msg.sender,
                register_pool_msg.pair,
                register_pool_msg.tx_id,
            )
        }
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
        ExecuteMsg::Swap(swap_msg) => {
            let amp_factor = AMP_FACTOR.load(deps.storage).unwrap_or(DEFAULT_AMP_FACTOR);
            execute_swap(
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
                SwapCalculationMethod::Stable(amp_factor),
                swap_msg.test_fail,
            )
        }
        ExecuteMsg::UpdateAdmin { admin, admin_type } => {
            update_admin(deps, env, info, &ADMIN, admin, admin_type)
        }
        ExecuteMsg::UpdateAmpFactor { amp_factor } => {
            update_amp_factor(deps, info, &ADMIN, &AMP_FACTOR, amp_factor)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query_state(deps),
        QueryMsg::GetAdmin {} => query_admin(deps),
        QueryMsg::SimulateSwap(simulate_swap_msg) => query_simulate_swap(
            deps,
            simulate_swap_msg.asset,
            simulate_swap_msg.asset_amount,
            simulate_swap_msg.swaps,
        ),
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
