use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, Uint128};
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

pub fn calc_fee(
    deps: &DepsMut,
    count: u128,
    free_limit: Option<u128>,
) -> Result<Uint128, ContractError> {
    let state = RATE_LIMIT_STATE.load(deps.storage)?;
    let free_limit = free_limit.unwrap_or(state.free_limit);

    // If count is less than or equal to free limit, return zero fee
    if count.le(&free_limit) {
        return Ok(Uint128::zero());
    }

    // If count is greater than free limit, calculate fee
    let fee_brackets = state.fee_brackets;

    // Loop from last to first fee bracket and return on the first that is less than count
    for bracket in fee_brackets.iter().rev() {
        if count >= bracket.threshold {
            return Ok(bracket.fee);
        }
    }

    Ok(Uint128::zero())
}
