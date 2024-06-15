use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::escrow::{AllowedTokenResponse, TokenIdResponse},
    token::TokenInfo,
};

use crate::state::{ALLOWED_DENOMS, STATE};

// New escrow query functions

// Returns the token id
pub fn query_token_id(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TokenIdResponse {
        token_id: state.token_id.id,
    })?)
}

// Returns the token id
pub fn query_token_allowed(deps: Deps, token: TokenInfo) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let mut response = AllowedTokenResponse { allowed: false };

    if state.token_id == token.get_token() {
        let registered_denom = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
        response.allowed = registered_denom.contains(&token.get_denom());
    }

    Ok(to_json_binary(&response)?)
}
