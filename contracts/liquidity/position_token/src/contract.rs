#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, DepsMut, Empty, Env, MessageInfo, Order, Response};
use cw2::set_contract_version;

use euclid::error::ContractError;

use crate::msg::{
    ExecuteMsg, InstantiateMsg, OwnerOfResponse, QueryMsg, StateResponse, TokenInfoResponse,
    TokensResponse,
};
use crate::state::{State, TokenInfo, ALL_TOKEN_SET, OWNER_TOKEN_SET, STATE, TOKENS};

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
    }
}

fn execute_mint(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,
    owner: String,
    token_uri: Option<String>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    ensure!(
        info.sender == state.minter || info.sender == state.admin,
        ContractError::Unauthorized {}
    );
    ensure!(
        !token_id.trim().is_empty(),
        ContractError::InvalidTokenID {}
    );
    ensure!(
        !TOKENS.has(deps.storage, &token_id),
        ContractError::TokenAlreadyExist {}
    );

    let owner_addr = deps.api.addr_validate(owner.as_str())?;
    TOKENS.save(
        deps.storage,
        &token_id,
        &TokenInfo {
            owner: owner_addr.clone(),
            token_uri,
        },
    )?;

    OWNER_TOKEN_SET.save(deps.storage, (&owner_addr, &token_id), &Empty {})?;
    ALL_TOKEN_SET.save(deps.storage, &token_id, &Empty {})?;

    state.total_tokens = state
        .total_tokens
        .checked_add(1)
        .ok_or_else(|| ContractError::new("total token overflow"))?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "mint")
        .add_attribute("token_id", token_id)
        .add_attribute("owner", owner_addr))
}

fn execute_burn(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    let token = TOKENS
        .may_load(deps.storage, &token_id)?
        .ok_or(ContractError::NotFound {
            msg: format!("token {token_id} not found"),
        })?;

    ensure!(
        info.sender == token.owner || info.sender == state.admin || info.sender == state.minter,
        ContractError::Unauthorized {}
    );

    TOKENS.remove(deps.storage, &token_id);
    OWNER_TOKEN_SET.remove(deps.storage, (&token.owner, &token_id));
    ALL_TOKEN_SET.remove(deps.storage, &token_id);

    state.total_tokens = state
        .total_tokens
        .checked_sub(1)
        .ok_or_else(|| ContractError::new("total token underflow"))?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "burn")
        .add_attribute("token_id", token_id))
}

fn execute_transfer(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,
    recipient: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let mut token = TOKENS
        .may_load(deps.storage, &token_id)?
        .ok_or(ContractError::NotFound {
            msg: format!("token {token_id} not found"),
        })?;

    ensure!(
        info.sender == token.owner || info.sender == state.admin,
        ContractError::Unauthorized {}
    );

    let recipient_addr = deps.api.addr_validate(recipient.as_str())?;
    ensure!(recipient_addr != token.owner, ContractError::SameAddress {});

    OWNER_TOKEN_SET.remove(deps.storage, (&token.owner, &token_id));
    OWNER_TOKEN_SET.save(deps.storage, (&recipient_addr, &token_id), &Empty {})?;

    token.owner = recipient_addr.clone();
    TOKENS.save(deps.storage, &token_id, &token)?;

    Ok(Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("token_id", token_id)
        .add_attribute("recipient", recipient_addr))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::OwnerOf { token_id } => query_owner_of(deps, token_id),
        QueryMsg::TokenInfo { token_id } => query_token_info(deps, token_id),
        QueryMsg::TokensByOwner { owner } => query_tokens_by_owner(deps, owner),
        QueryMsg::AllTokens {} => query_all_tokens(deps),
        QueryMsg::State {} => query_state(deps),
    }
}

fn query_owner_of(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let token = TOKENS
        .may_load(deps.storage, &token_id)?
        .ok_or(ContractError::NotFound {
            msg: format!("token {token_id} not found"),
        })?;

    Ok(to_json_binary(&OwnerOfResponse {
        owner: token.owner.to_string(),
    })?)
}

fn query_token_info(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let token = TOKENS
        .may_load(deps.storage, &token_id)?
        .ok_or(ContractError::NotFound {
            msg: format!("token {token_id} not found"),
        })?;

    Ok(to_json_binary(&TokenInfoResponse {
        token_id,
        owner: token.owner.to_string(),
        token_uri: token.token_uri,
    })?)
}

fn query_tokens_by_owner(deps: Deps, owner: String) -> Result<Binary, ContractError> {
    let owner_addr = deps.api.addr_validate(owner.as_str())?;
    let tokens: Vec<String> = OWNER_TOKEN_SET
        .prefix(&owner_addr)
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(to_json_binary(&TokensResponse { tokens })?)
}

fn query_all_tokens(deps: Deps) -> Result<Binary, ContractError> {
    let tokens: Vec<String> = ALL_TOKEN_SET
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(to_json_binary(&TokensResponse { tokens })?)
}

fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        name: state.name,
        symbol: state.symbol,
        minter: state.minter,
        admin: state.admin,
        total_tokens: state.total_tokens,
    })?)
}
