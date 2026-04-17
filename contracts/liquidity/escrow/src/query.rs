use cosmwasm_std::{to_json_binary, Binary, Deps, Order};
use cw_storage_plus::Bound;
use euclid::{
    error::ContractError,
    msgs::escrow::{
        AllDenomBalancesResponse, AllowedDenomsResponse, AllowedTokenResponse, DenomBalance,
        DenomBalanceResponse, DisallowedDenomsResponse, StateResponse, TokenIdResponse,
    },
    token::TokenType,
    utils::pagination::{Pagination, DEFAULT_PAGINATION_LIMIT, DEFAULT_PAGINATION_SKIP},
};

use crate::state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT, DISALLOWED_DENOMS, STATE};

// New escrow query functions

// Returns the token id
pub fn query_token_id(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TokenIdResponse {
        token_id: state.token_id.to_string(),
    })?)
}

// Returns allowed tokens
pub fn query_token_allowed(deps: Deps, denom: TokenType) -> Result<Binary, ContractError> {
    let registered_denom = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
    let response = AllowedTokenResponse {
        allowed: registered_denom.contains(&denom),
    };

    Ok(to_json_binary(&response)?)
}

// Returns the allowed denoms
pub fn query_allowed_denoms(deps: Deps) -> Result<Binary, ContractError> {
    let denoms = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
    let response = AllowedDenomsResponse { denoms };

    Ok(to_json_binary(&response)?)
}

pub fn query_disallowed_denoms(deps: Deps) -> Result<Binary, ContractError> {
    let denoms = DISALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
    Ok(to_json_binary(&DisallowedDenomsResponse { denoms })?)
}

pub fn query_denom_balance(deps: Deps, denom: String) -> Result<Binary, ContractError> {
    let amount = DENOM_TO_AMOUNT
        .may_load(deps.storage, denom.clone())?
        .unwrap_or_default();
    Ok(to_json_binary(&DenomBalanceResponse { denom, amount })?)
}

pub fn query_all_denom_balances(
    deps: Deps,
    pagination: Option<Pagination<String>>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min: start,
        max: end,
        skip,
        limit,
    } = pagination.unwrap_or_default();

    let start = start.map(Bound::inclusive);
    let end = end.map(Bound::exclusive);

    let balances = DENOM_TO_AMOUNT
        .range(deps.storage, start, end, Order::Ascending)
        .skip(skip.unwrap_or(DEFAULT_PAGINATION_SKIP) as usize)
        .take(limit.unwrap_or(DEFAULT_PAGINATION_LIMIT) as usize)
        .map(|item| {
            let (denom, amount) = item?;
            Ok(DenomBalance { denom, amount })
        })
        .collect::<Result<Vec<_>, cosmwasm_std::StdError>>()?;
    Ok(to_json_binary(&AllDenomBalancesResponse { balances })?)
}

// Returns the allowed denoms
pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let response = StateResponse {
        token: state.token_id,
        factory_address: state.factory_address,
        total_amount: state.total_amount,
    };

    Ok(to_json_binary(&response)?)
}
