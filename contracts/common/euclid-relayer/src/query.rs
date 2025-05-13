use cosmwasm_std::Deps;
use euclid::error::ContractError;
use relayer::msgs::State;

use crate::state::{NONCES, STATE};

pub fn get_state(deps: &Deps) -> Result<State, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

pub fn nonce_relayed(deps: &Deps, nonce: String) -> Result<bool, ContractError> {
    Ok(NONCES.has(deps.storage, nonce))
}
