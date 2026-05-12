use crate::state::{DEFAULT_RELEASE_FEE, RELEASE_FEES};
use cosmwasm_std::{DepsMut, Uint256};
use euclid::{chain::ChainUid, token::Token};

/// Default release fee is 0 if not set
pub fn default_release_fee(deps: &mut DepsMut) -> Uint256 {
    DEFAULT_RELEASE_FEE
        .load(deps.storage)
        .unwrap_or(Uint256::zero())
}

pub fn get_release_fee_storage(deps: &mut DepsMut, token: &Token, chain_uid: &ChainUid) -> Uint256 {
    RELEASE_FEES
        .load(deps.storage, (token.clone(), chain_uid.clone()))
        .unwrap_or(default_release_fee(deps))
}
