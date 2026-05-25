#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::{
    error::ContractError,
    msgs::pool_factory::{ExecuteMsg, InstantiateMsg, QueryMsg},
};

use crate::{
    execute::{
        ack::on_pool_ack,
        clp::on_request_concentrated_pool_creation,
        cp::{on_add_liquidity, on_remove_liquidity, on_request_pool_creation},
        migrate::migrate_accept_pool_state,
    },
    query::{
        get_concentrated_vlp, get_lp_token, get_main_factory_address, get_position_token_contract,
        get_vlp,
    },
    reply::{on_lp_instantiate_reply, LP_INSTANTIATE_REPLY_ID},
    state::MAIN_FACTORY_ADDRESS,
};

pub(crate) const CONTRACT_NAME: &str = "crates.io:pool_factory";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let main_factory = deps.api.addr_validate(&msg.main_factory_address)?;
    MAIN_FACTORY_ADDRESS.save(deps.storage, &main_factory)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("main_factory", main_factory))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::OnRequestPoolCreation {
            tx_id,
            sender,
            pair_with_denom_and_amount,
            pool_config,
            lp_token_name,
            lp_token_symbol,
            lp_token_decimal,
            lp_token_marketing,
            slippage_tolerance_bps,
            cross_chain_config,
        } => on_request_pool_creation(
            deps,
            env,
            info,
            tx_id,
            sender,
            pair_with_denom_and_amount,
            pool_config,
            lp_token_name,
            lp_token_symbol,
            lp_token_decimal,
            lp_token_marketing,
            slippage_tolerance_bps,
            cross_chain_config,
        ),
        ExecuteMsg::OnAddLiquidity {
            tx_id,
            sender,
            pair_with_denom_and_amount,
            slippage_tolerance_bps,
            cross_chain_config,
        } => on_add_liquidity(
            deps,
            env,
            info,
            tx_id,
            sender,
            pair_with_denom_and_amount,
            slippage_tolerance_bps,
            cross_chain_config,
        ),
        ExecuteMsg::OnRemoveLiquidity {
            tx_id,
            sender,
            pair,
            lp_allocation,
            lp_token,
            recipient,
            cross_chain_config,
        } => on_remove_liquidity(
            deps,
            env,
            info,
            tx_id,
            sender,
            pair,
            lp_allocation,
            lp_token,
            recipient,
            cross_chain_config,
        ),
        ExecuteMsg::OnRequestConcentratedPoolCreation {
            tx_id,
            sender,
            pair_with_denom_and_amount,
            pool_key,
            slippage_tolerance_bps,
            initial_tick,
            cross_chain_config,
        } => on_request_concentrated_pool_creation(
            deps,
            env,
            info,
            tx_id,
            sender,
            pair_with_denom_and_amount,
            pool_key,
            slippage_tolerance_bps,
            initial_tick,
            cross_chain_config,
        ),
        ExecuteMsg::OnPoolAck {
            original_msg,
            ack,
            is_native,
        } => on_pool_ack(deps, env, info, original_msg, ack, is_native),
        ExecuteMsg::MigrateAcceptPoolState {
            pair_to_vlp,
            vlp_to_lp_token,
            concentrated_vlps,
            position_token_contract,
        } => migrate_accept_pool_state(
            deps,
            env,
            info,
            pair_to_vlp,
            vlp_to_lp_token,
            concentrated_vlps,
            position_token_contract,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetVlp { pair } => get_vlp(deps, pair),
        QueryMsg::GetLpToken { vlp } => get_lp_token(deps, vlp),
        QueryMsg::GetMainFactoryAddress {} => get_main_factory_address(deps),
        QueryMsg::GetConcentratedVlp { pool_key } => get_concentrated_vlp(deps, pool_key),
        QueryMsg::GetPositionTokenContract {} => get_position_token_contract(deps),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        LP_INSTANTIATE_REPLY_ID => on_lp_instantiate_reply(deps, msg),
        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {id}"
        )))),
    }
}
