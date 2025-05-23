use cosmwasm_std::{Binary, Deps};
use euclid::{
    error::ContractError,
    msgs::claimer::{Claim, State},
};

use crate::state::{CLAIMS, SENDER_CLAIMS, STATE, USER_CLAIMS};

pub fn get_state(deps: &Deps) -> Result<State, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

pub fn get_sender_claims(deps: &Deps, sender: String) -> Result<Vec<u128>, ContractError> {
    let sender_claims = SENDER_CLAIMS.load(deps.storage, sender)?;
    Ok(sender_claims)
}

pub fn get_user_claims(deps: &Deps, pub_key: Binary) -> Result<Vec<u128>, ContractError> {
    let user_claims = USER_CLAIMS.load(deps.storage, pub_key.to_string())?;
    Ok(user_claims)
}

pub fn get_claim(deps: &Deps, claim_id: u128) -> Result<Claim, ContractError> {
    let claim = CLAIMS.load(deps.storage, claim_id)?;
    Ok(claim)
}
