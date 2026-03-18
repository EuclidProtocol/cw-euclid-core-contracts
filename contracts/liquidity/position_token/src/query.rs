use crate::state::{ALL_TOKEN_SET, OWNER_TOKEN_SET, STATE, TOKENS};
use cosmwasm_std::{to_json_binary, Binary, Deps, Order};
use euclid::error::ContractError;
use euclid::msgs::position_token::{
    OwnerOfResponse, StateResponse, TokenInfoResponse, TokensResponse,
};

pub(crate) fn query_owner_of(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let token = TOKENS
        .may_load(deps.storage, &token_id)?
        .ok_or(ContractError::NotFound {
            msg: format!("token {token_id} not found"),
        })?;

    Ok(to_json_binary(&OwnerOfResponse {
        owner: token.owner.to_string(),
    })?)
}

pub(crate) fn query_token_info(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
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

pub(crate) fn query_tokens_by_owner(deps: Deps, owner: String) -> Result<Binary, ContractError> {
    let owner_addr = deps.api.addr_validate(owner.as_str())?;
    let tokens: Vec<String> = OWNER_TOKEN_SET
        .prefix(&owner_addr)
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(to_json_binary(&TokensResponse { tokens })?)
}

pub(crate) fn query_all_tokens(deps: Deps) -> Result<Binary, ContractError> {
    let tokens: Vec<String> = ALL_TOKEN_SET
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(to_json_binary(&TokensResponse { tokens })?)
}

pub(crate) fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        name: state.name,
        symbol: state.symbol,
        minter: state.minter,
        admin: state.admin,
        total_tokens: state.total_tokens,
    })?)
}
