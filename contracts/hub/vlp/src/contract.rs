#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, Uint128};
use cw2::set_contract_version;
use euclid::common::{generate_instantiate2_message, get_new_addr};

use crate::reply::{NEXT_SWAP_REPLY_ID, VCOIN_TRANSFER_REPLY_ID};
use crate::state::{State, STATE};
use crate::{execute, reply};
use euclid::error::ContractError;
use euclid::msgs::vlp::{ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::query::{query_all_pools, query_fee, query_liquidity, query_pool, query_simulate_swap};
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

    // Get Instantiate2 address of cw20
    let cw20_addr = get_new_addr(
        deps.api,
        msg.cw20_code_id,
        env.contract.address.clone().into_string(),
        &deps.querier,
    )?;

    let state = State {
        pair: msg.pair,
        vcoin: msg.vcoin,
        router: info.sender.to_string(),
        // TODO handle None instance
        cw20: cw20_addr.unwrap().into_string(),
        fee: msg.fee,
        last_updated: 0,
        total_reserve_1: Uint128::zero(),
        total_reserve_2: Uint128::zero(),
        total_lp_tokens: Uint128::zero(),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    let response = msg.execute.map_or(Ok(Response::default()), |execute_msg| {
        execute(deps, env.clone(), info.clone(), execute_msg)
    })?;
    // TODO create idx for this instantiation process
    let instaniate2_msg = generate_instantiate2_message(
        msg.cw20_code_id,
        env.clone().contract.address.into_string(),
        1,
    )?;

    Ok(response
        .add_submessage(instaniate2_msg)
        .add_attribute("method", "instantiate")
        .add_attribute("vlp_address", env.clone().contract.address.to_string())
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterPool {
            chain_id,
            pair_info,
        } => execute::register_pool(deps, env, chain_id, pair_info),
        ExecuteMsg::Swap {
            to_chain_id,
            to_address,
            asset_in,
            amount_in,
            min_token_out,
            swap_id,
            next_swaps,
        } => execute::execute_swap(
            deps,
            env,
            to_chain_id,
            to_address,
            asset_in,
            amount_in,
            min_token_out,
            swap_id,
            next_swaps,
        ),
        ExecuteMsg::AddLiquidity {
            chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            outpost_sender,
        } => execute::add_liquidity(
            deps,
            chain_id,
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            outpost_sender,
        ),
        ExecuteMsg::RemoveLiquidity {
            chain_id,
            lp_allocation,
            outpost_sender,
        } => execute::remove_liquidity(deps, chain_id, lp_allocation, outpost_sender),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::SimulateSwap {
            asset,
            asset_amount,
            swaps,
        } => query_simulate_swap(deps, asset, asset_amount, swaps),
        QueryMsg::Liquidity {} => query_liquidity(deps),
        QueryMsg::Fee {} => query_fee(deps),
        QueryMsg::Pool { chain_id } => query_pool(deps, chain_id),
        QueryMsg::GetAllPools {} => query_all_pools(deps),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        VCOIN_TRANSFER_REPLY_ID => reply::on_vcoin_transfer_reply(deps, msg),
        NEXT_SWAP_REPLY_ID => reply::on_next_swap_reply(deps, msg),

        id => Err(ContractError::Generic {
            err: format!("Unknown reply id: {id}"),
        }),
    }
}
