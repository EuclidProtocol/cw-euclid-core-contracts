use cosmwasm_std::Deps;
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::{NonceRelayedResponse, State};

use crate::state::{NONCES, STATE};

pub fn get_state(deps: &Deps) -> Result<State, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

pub fn get_nonce(deps: &Deps, nonce: String) -> Result<NonceRelayedResponse, ContractError> {
    let nonce = NONCES.load(deps.storage, (nonce.clone(), nonce.clone()))?;
    Ok(NonceRelayedResponse { height: nonce })
}
