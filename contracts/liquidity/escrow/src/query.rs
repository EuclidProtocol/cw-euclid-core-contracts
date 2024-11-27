use anybuf::Anybuf;
use cosmwasm_std::{
    to_binary, to_vec, Binary, ContractResult, Deps, QuerierWrapper, StdError, StdResult,
    SystemResult,
};
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
    Ok(to_binary(&TokenIdResponse {
        token_id: state.token_id.to_string(),
    })?)
}

// Returns allowed tokens
pub fn query_token_allowed(deps: Deps, denom: TokenType) -> Result<Binary, ContractError> {
    let registered_denom = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
    let response = AllowedTokenResponse {
        allowed: registered_denom.contains(&denom),
    };

    Ok(to_binary(&response)?)
}

// Returns the allowed denoms
pub fn query_allowed_denoms(deps: Deps) -> Result<Binary, ContractError> {
    let denoms = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();
    let response = AllowedDenomsResponse { denoms };

    Ok(to_binary(&response)?)
}

// Returns the allowed denoms
pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let response = StateResponse {
        token: state.token_id,
        factory_address: state.factory_address,
        total_amount: state.total_amount,
    };

    Ok(to_binary(&response)?)
}

pub fn get_contract_code_hash(
    querier: QuerierWrapper,
    contract_address: String,
) -> StdResult<String> {
    let code_hash_query: cosmwasm_std::QueryRequest<cosmwasm_std::Empty> =
        cosmwasm_std::QueryRequest::Stargate {
            path: "/secret.compute.v1beta1.Query/CodeHashByContractAddress".into(),
            data: Binary(Anybuf::new().append_string(1, contract_address).into_vec()),
        };

    let raw = to_vec(&code_hash_query).map_err(|serialize_err| {
        StdError::generic_err(format!("Serializing QueryRequest: {}", serialize_err))
    })?;

    let code_hash = match querier.raw_query(&raw) {
        SystemResult::Err(system_err) => Err(StdError::generic_err(format!(
            "Querier system error: {}",
            system_err
        ))),
        SystemResult::Ok(ContractResult::Err(contract_err)) => Err(StdError::generic_err(format!(
            "Querier contract error: {}",
            contract_err
        ))),
        SystemResult::Ok(ContractResult::Ok(value)) => Ok(value),
    }?;

    // Remove the "\n@" if it exists at the start of the code_hash
    let mut code_hash_str = String::from_utf8(code_hash.to_vec())
        .map_err(|err| StdError::generic_err(format!("Invalid UTF-8 sequence: {}", err)))?;

    if code_hash_str.starts_with("\n@") {
        code_hash_str = code_hash_str.trim_start_matches("\n@").to_string();
    }

    Ok(code_hash_str)
}
