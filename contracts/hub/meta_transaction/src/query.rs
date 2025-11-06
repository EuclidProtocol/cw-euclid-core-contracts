use cosmwasm_std::Deps;
use euclid::chain::ChainUid;
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::State;

use crate::state::{NONCES, STATE};

pub fn get_state(deps: &Deps) -> Result<State, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

pub fn nonce_relayed(
    deps: &Deps,
    chain_uid: ChainUid,
    address: String,
    nonce: String,
) -> Result<bool, ContractError> {
    // Create sender key: chainuid:address
    let sender_key = format!("{}:{}", chain_uid.as_str(), address);
    Ok(NONCES.has(deps.storage, (sender_key, nonce)))
}
