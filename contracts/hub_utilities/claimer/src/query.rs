use cosmwasm_std::{Binary, Deps, Order};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::claimer::msg::{Claim, State},
};

use crate::state::{CLAIMS, STATE};

pub fn get_state(deps: &Deps) -> Result<State, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

pub fn get_claims_by_sender(
    deps: &Deps,
    sender: CrossChainUser,
    limit: u64,
    offset: u64,
) -> Result<Vec<(u128, Claim)>, ContractError> {
    let sender_claims = CLAIMS
        .range(deps.storage, None, None, Order::Ascending)
        .filter(|claim| {
            if let Ok(claim) = claim {
                claim.1.sender == sender
            } else {
                false
            }
        })
        .take(limit as usize)
        .skip(offset as usize)
        .flatten()
        .collect::<Vec<_>>();
    Ok(sender_claims)
}

pub fn get_claims_by_claimer_pubkey(
    deps: &Deps,
    pub_key: Binary,
    limit: u64,
    offset: u64,
) -> Result<Vec<(u128, Claim)>, ContractError> {
    let user_claims = CLAIMS
        .range(deps.storage, None, None, Order::Ascending)
        .filter(|claim| {
            if let Ok(claim) = claim {
                claim.1.claimer_pubkey == pub_key
            } else {
                false
            }
        })
        .take(limit as usize)
        .skip(offset as usize)
        .flatten()
        .collect::<Vec<_>>();
    Ok(user_claims)
}

pub fn get_claim(deps: &Deps, claim_id: u128) -> Result<Claim, ContractError> {
    let claim = CLAIMS.load(deps.storage, claim_id)?;
    Ok(claim)
}

pub fn get_claims_by_group_id(
    deps: &Deps,
    group_id: String,
    limit: u64,
    offset: u64,
) -> Result<Vec<(u128, Claim)>, ContractError> {
    let user_claims = CLAIMS
        .range(deps.storage, None, None, Order::Ascending)
        .filter(|claim| {
            if let Ok(claim) = claim {
                claim.1.claim_group_id == Some(group_id.clone())
            } else {
                false
            }
        })
        .take(limit as usize)
        .skip(offset as usize)
        .flatten()
        .collect::<Vec<_>>();
    Ok(user_claims)
}

pub fn get_claim_by_pseudo_claim_id(
    deps: &Deps,
    pseudo_claim_id: String,
) -> Result<(u128, Claim), ContractError> {
    let claim = CLAIMS
        .range(deps.storage, None, None, Order::Ascending)
        .find(|claim| {
            if let Ok(claim) = claim {
                claim.1.pseudo_claim_id == Some(pseudo_claim_id.clone())
            } else {
                false
            }
        })
        .ok_or(ContractError::NotFound {
            msg: "Claim not found".to_string(),
        })??;
    Ok((claim.0, claim.1))
}
