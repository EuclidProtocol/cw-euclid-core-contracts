use cosmwasm_std::{Deps, Order};
use euclid::{admin::EuclidAdmin, error::ContractError};
use relayer::{msgs::State, ValidatorsResponse, ValidatorsResponseItem};

use crate::state::{ADMIN, NONCES, STATE, VALIDATORS};

pub fn get_state(deps: &Deps) -> Result<State, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

pub fn get_admin(deps: &Deps) -> Result<EuclidAdmin, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    Ok(admin)
}

pub fn get_nonce_relayed(deps: &Deps, nonce: String) -> Result<bool, ContractError> {
    Ok(NONCES.has(deps.storage, nonce))
}

pub fn get_validators(deps: &Deps) -> Result<ValidatorsResponse, ContractError> {
    let mut validators = vec![];
    let iter = VALIDATORS.range(deps.storage, None, None, Order::Ascending);
    for v in iter {
        let (chain_uid, chain_validators) = v?;
        for validator in chain_validators.iter() {
            validators.push(ValidatorsResponseItem {
                validator: validator.clone(),
                chain_uid: chain_uid.clone(),
            });
        }
    }
    Ok(ValidatorsResponse { validators })
}
