use cosmwasm_std::{ensure, DepsMut, Empty, MessageInfo, Response};
use euclid::error::ContractError;

use crate::state::{TokenInfo, ALL_TOKEN_SET, OWNER_TOKEN_SET, STATE, TOKENS};

pub(crate) fn execute_mint(
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

pub(crate) fn execute_update_state(
    deps: DepsMut,
    info: MessageInfo,
    admin: Option<String>,
    minter: Option<String>,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;

    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    if let Some(admin) = admin {
        state.admin = deps.api.addr_validate(&admin)?;
    }

    if let Some(minter) = minter {
        state.minter = deps.api.addr_validate(&minter)?;
    }

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "update_state")
        .add_attribute("admin", state.admin)
        .add_attribute("minter", state.minter))
}

pub(crate) fn execute_burn(
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

pub(crate) fn execute_transfer(
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
