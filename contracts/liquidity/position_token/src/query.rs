use crate::state::{ALL_TOKEN_SET, OWNER_TOKEN_SET, STATE, TOKENS};
use cosmwasm_std::{to_json_binary, Binary, Deps, Order};
use cw_storage_plus::Bound;
use euclid::error::ContractError;
use euclid::msgs::position_token::{
    OwnerOfResponse, StateResponse, TokenInfoResponse, TokensResponse,
};

const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 100;

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

pub(crate) fn query_tokens_by_owner(
    deps: Deps,
    owner: String,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<Binary, ContractError> {
    let owner_addr = deps.api.addr_validate(owner.as_str())?;
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.as_deref().map(Bound::exclusive);

    let tokens: Vec<String> = OWNER_TOKEN_SET
        .prefix(&owner_addr)
        .keys(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(to_json_binary(&TokensResponse { tokens })?)
}

pub(crate) fn query_all_tokens(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<Binary, ContractError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.as_deref().map(Bound::exclusive);

    let tokens: Vec<String> = ALL_TOKEN_SET
        .keys(deps.storage, start, None, Order::Ascending)
        .take(limit)
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
