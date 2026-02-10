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
    // User free limit provides custom rate limit for a user
    let user_free_limit = USER_FREE_LIMIT.may_load(deps.storage, sender.clone())?;

    let pending_count = USER_PENDING_PACKETS_COUNT
        .load(deps.storage, sender.clone())
        .unwrap_or(0);

    if let Some(user_free_limit) = user_free_limit {
        ensure!(
            pending_count < user_free_limit,
            ContractError::RateLimitExceeded {
                limit: user_free_limit,
                actual: pending_count,
            }
        );
    } else {
        let state = RATE_LIMIT_STATE.load(deps.storage)?;
        ensure!(
            pending_count < state.free_limit,
            ContractError::RateLimitExceeded {
                limit: state.free_limit,
                actual: pending_count,
            }
        );
    }
    Ok(())
}

mod tests {
    use cosmwasm_std::testing::mock_dependencies;

    use super::*;

    #[test]
    fn test_ensure_rate_limit_pass() {
        let mut deps = mock_dependencies();
        let sender = Addr::unchecked("sender");
        USER_FREE_LIMIT
            .save(deps.as_mut().storage, sender.clone(), &100)
            .unwrap();
        USER_PENDING_PACKETS_COUNT
            .save(deps.as_mut().storage, sender.clone(), &99)
            .unwrap();
        RATE_LIMIT_STATE
            .save(
                deps.as_mut().storage,
                &RateLimitState {
                    free_limit: 100,
                    fee_brackets: vec![],
                },
            )
            .unwrap();
        assert!(
            ensure_rate_limit_exceeded(&deps.as_mut(), sender).is_ok(),
            "Rate limit should pass"
        );
    }

    #[test]
    fn test_ensure_rate_limit_exceeded() {
        let mut deps = mock_dependencies();
        let sender = Addr::unchecked("sender");
        USER_FREE_LIMIT
            .save(deps.as_mut().storage, sender.clone(), &100)
            .unwrap();
        USER_PENDING_PACKETS_COUNT
            .save(deps.as_mut().storage, sender.clone(), &100)
            .unwrap();
        RATE_LIMIT_STATE
            .save(
                deps.as_mut().storage,
                &RateLimitState {
                    free_limit: 100,
                    fee_brackets: vec![],
                },
            )
            .unwrap();
        assert!(
            ensure_rate_limit_exceeded(&deps.as_mut(), sender).is_err(),
            "Rate limit should be exceeded"
        );
    }

    #[test]
    fn test_ensure_rate_limit_exceeded_no_user_free_limit() {
        let mut deps = mock_dependencies();
        let sender = Addr::unchecked("sender");
        USER_PENDING_PACKETS_COUNT
            .save(deps.as_mut().storage, sender.clone(), &100)
            .unwrap();
        RATE_LIMIT_STATE
            .save(
                deps.as_mut().storage,
                &RateLimitState {
                    free_limit: 10,
                    fee_brackets: vec![],
                },
            )
            .unwrap();
        assert!(
            ensure_rate_limit_exceeded(&deps.as_mut(), sender).is_err(),
            "Rate limit should be exceeded because free limit is 10 and pending count is 100"
        );
    }

    #[test]
    fn test_ensure_rate_limit_exceeded_both_free_limit_and_user_free_limit_are_set() {
        let mut deps = mock_dependencies();
        let sender = Addr::unchecked("sender");
        USER_FREE_LIMIT
            .save(deps.as_mut().storage, sender.clone(), &10)
            .unwrap();
        USER_PENDING_PACKETS_COUNT
            .save(deps.as_mut().storage, sender.clone(), &50)
            .unwrap();
        RATE_LIMIT_STATE
            .save(
                deps.as_mut().storage,
                &RateLimitState {
                    free_limit: 100,
                    fee_brackets: vec![],
                },
            )
            .unwrap();
        assert!(
            ensure_rate_limit_exceeded(&deps.as_mut(), sender).is_err(),
            "Rate limit should be exceeded even if free limit is more than user free limit"
        );
    }
}
