#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError};
use cw2::set_contract_version;
use euclid::error::ContractError;

use crate::reply::{
    self, ADD_LIQUIDITY_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID,
    VLP_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
};
use crate::state::{State, STATE};
use crate::{execute, query};
use euclid::msgs::router::{ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:factory";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        vlp_code_id: msg.vlp_code_id,
        admin: info.sender.to_string(),
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", env.contract.address))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateVLPCodeId { new_vlp_code_id } => {
            execute::execute_update_vlp_code_id(deps, info, new_vlp_code_id)
        }
        ExecuteMsg::RegisterFactory { channel, timeout } => {
            execute::execute_register_factory(deps, env, info, channel, timeout)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => query::query_state(deps),
        QueryMsg::GetChain { chain_id } => query::query_chain(deps, chain_id),
        QueryMsg::GetAllChains {} => query::query_all_chains(deps),
        QueryMsg::GetVlp { token_1, token_2 } => query::query_vlp(deps, token_1, token_2),
        QueryMsg::GetAllVlps {} => query::query_all_vlps(deps),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        VLP_INSTANTIATE_REPLY_ID => reply::on_vlp_instantiate_reply(deps, msg),
        VLP_POOL_REGISTER_REPLY_ID => reply::on_pool_register_reply(deps, msg),
        ADD_LIQUIDITY_REPLY_ID => reply::on_add_liquidity_reply(deps, msg),
        REMOVE_LIQUIDITY_REPLY_ID => reply::on_remove_liquidity_reply(deps, msg),
        SWAP_REPLY_ID => reply::on_swap_reply(deps, msg),
        id => Err(ContractError::Std(StdError::generic_err(format!(
            "Unknown reply id: {}",
            id
        )))),
    }
}
