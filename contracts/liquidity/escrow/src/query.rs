use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::escrow::{AllowedDenomsResponse, AllowedTokenResponse, StateResponse, TokenIdResponse},
    token::TokenType,
};

use crate::state::{ALLOWED_DENOMS, STATE};

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
