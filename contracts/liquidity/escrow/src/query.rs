use cosmwasm_std::{to_json_binary, Binary, Deps};
use euclid::{error::ContractError, msgs::escrow::TokenIdResponse};

use crate::state::STATE;

// New escrow query functions

// Returns the token id
pub fn query_token_id(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TokenIdResponse {
        token_id: state.token_id.id,
    })?)
}
