#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;

use euclid::error::ContractError;

use crate::execute::{execute_burn, execute_mint, execute_transfer, execute_update_state};
use crate::query::{
    query_all_tokens, query_owner_of, query_state, query_token_info, query_tokens_by_owner,
};
use crate::state::{State, STATE};
use euclid::msgs::position_token::{ExecuteMsg, InstantiateMsg, QueryMsg};

const CONTRACT_NAME: &str = "crates.io:position_token";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    ensure!(
        !msg.name.trim().is_empty(),
        ContractError::new("name cannot be empty")
    );
    ensure!(
        !msg.symbol.trim().is_empty(),
        ContractError::new("symbol cannot be empty")
    );

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    STATE.save(
        deps.storage,
        &State {
            name: msg.name,
            symbol: msg.symbol,
            minter: msg.minter,
            admin: msg.admin,
            total_tokens: 0,
        },
    )?;

    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Mint {
            token_id,
            owner,
            token_uri,
        } => execute_mint(deps, info, token_id, owner, token_uri),
        ExecuteMsg::Burn { token_id } => execute_burn(deps, info, token_id),
        ExecuteMsg::Transfer {
            token_id,
            recipient,
        } => execute_transfer(deps, info, token_id, recipient),
        ExecuteMsg::UpdateState { admin, minter } => {
            execute_update_state(deps, info, admin, minter)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::OwnerOf { token_id } => query_owner_of(deps, token_id),
        QueryMsg::TokenInfo { token_id } => query_token_info(deps, token_id),
        QueryMsg::TokensByOwner { owner, pagination } => {
            query_tokens_by_owner(deps, owner, pagination)
        }
        QueryMsg::AllTokens { pagination } => query_all_tokens(deps, pagination),
        QueryMsg::State {} => query_state(deps),
    }
}
