use cosmwasm_std::Deps;
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::msg::{NonceRelayedResponse, StateResponse};

use crate::state::{ADMIN, NONCES, STATE};

pub fn get_state(deps: &Deps) -> Result<StateResponse, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admin = ADMIN.load(deps.storage)?;
    Ok(StateResponse {
        router_contract: state.router_contract,
        admin,
    })
}

pub fn get_nonce(deps: &Deps, nonce: String) -> Result<NonceRelayedResponse, ContractError> {
    let nonce = NONCES.load(deps.storage, (nonce.clone(), nonce.clone()))?;
    Ok(NonceRelayedResponse { height: nonce })
}
