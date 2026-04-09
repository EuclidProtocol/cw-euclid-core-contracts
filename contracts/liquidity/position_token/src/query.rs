use crate::state::{OWNER_TOKEN_SET, POSITION_INFO, STATE, TOKENS};
use cosmwasm_std::{to_json_binary, Binary, Deps, Order};
use cw_storage_plus::Bound;
use euclid::error::ContractError;
use euclid::msgs::position_token::{
    OwnerOfResponse, PositionInfoResponse, StateResponse, TokenInfoResponse, TokensResponse,
};
use euclid::utils::pagination::Pagination;

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
        owner: token.owner.to_string(),
        token_uri: token.token_uri,
    })?)
}

pub(crate) fn query_position_info(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let position =
        POSITION_INFO
            .may_load(deps.storage, &token_id)?
            .ok_or(ContractError::NotFound {
                msg: format!("token {token_id} not found"),
            })?;

    Ok(to_json_binary(&PositionInfoResponse {
        liquidity: position.liquidity,
    })?)
}

pub(crate) fn query_tokens_by_owner(
    deps: Deps,
    owner: String,
    pagination: Pagination<String>,
) -> Result<Binary, ContractError> {
    let owner_addr = deps.api.addr_validate(owner.as_str())?;
    let min = pagination.min.as_deref().map(Bound::inclusive);
    let max = pagination.max.as_deref().map(Bound::inclusive);

    let tokens: Vec<String> = OWNER_TOKEN_SET
        .prefix(&owner_addr)
        .keys(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(to_json_binary(&TokensResponse { tokens })?)
}

pub(crate) fn query_all_tokens(
    deps: Deps,
    pagination: Pagination<String>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.as_deref().map(Bound::inclusive);
    let max = pagination.max.as_deref().map(Bound::inclusive);

    let tokens: Vec<String> = TOKENS
        .keys(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(to_json_binary(&TokensResponse { tokens })?)
}

pub(crate) fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        name: state.name,
        symbol: state.symbol,
        factory: state.factory,
        vlp_address: state.vlp_address,
        total_tokens: state.total_tokens,
    })?)
}
