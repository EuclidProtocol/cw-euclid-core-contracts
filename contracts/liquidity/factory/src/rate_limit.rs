use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Addr, DepsMut, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::error::ContractError;

#[cw_serde]
pub struct FeeBracket {
    pub threshold: u128,
    pub fee: Uint128,
}

#[cw_serde]
pub struct RateLimitState {
    pub free_limit: u128,
    pub fee_brackets: Vec<FeeBracket>,
}
pub const RATE_LIMIT_STATE: Item<RateLimitState> = Item::new("rate_limit_state");

pub const USER_FREE_LIMIT: Map<Addr, u128> = Map::new("user_free_limit");
pub const USER_TOTAL_PACKETS_COUNT: Map<Addr, u128> = Map::new("user_total_packets_count");

pub const USER_PENDING_PACKETS_COUNT: Map<Addr, u128> = Map::new("user_pending_packets_count");

pub fn ensure_rate_limit_exceeded(deps: &DepsMut, sender: Addr) -> Result<(), ContractError> {
    let state = RATE_LIMIT_STATE.load(deps.storage)?;
    let user_free_limit = USER_FREE_LIMIT.may_load(deps.storage, sender.clone())?;

    let pending_count = USER_PENDING_PACKETS_COUNT
        .load(deps.storage, sender.clone())
        .unwrap_or(0);

    if let Some(user_free_limit) = user_free_limit {
        ensure!(
            pending_count < user_free_limit,
            ContractError::RateLimitExceeded {}
        );
    } else {
        ensure!(
            pending_count < state.free_limit,
            ContractError::RateLimitExceeded {}
        );
    }
    Ok(())
}
