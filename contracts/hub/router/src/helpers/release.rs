use crate::state::{DEFAULT_RELEASE_FEE, RELEASE_FEES};
use cosmwasm_std::{Decimal, DepsMut, Uint128};
use euclid::{chain::ChainUid, error::ContractError, token::Token};

/// Default release fee is 0% if not set
pub fn default_release_fee(deps: &mut DepsMut) -> Decimal {
    DEFAULT_RELEASE_FEE
        .load(deps.storage)
        .unwrap_or(Decimal::zero())
}

pub fn get_release_fee_storage(deps: &mut DepsMut, token: &Token, chain_uid: &ChainUid) -> Decimal {
    RELEASE_FEES
        .load(deps.storage, (token.clone(), chain_uid.clone()))
        .unwrap_or(default_release_fee(deps))
}

/// Calculate the release fee for a given amount
pub fn calculate_release_fee(
    amount: Uint128,
    release_fee: Decimal,
) -> Result<Uint128, ContractError> {
    Ok(release_fee.checked_mul(Decimal::new(amount))?.atomics())
}

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_calculate_release_fee() {
        // 1,000,000
        let amount = Uint128::from(1_000_000u128);
        // 0.1%
        let release_fee = Decimal::from_ratio(1u128, 1000u128);
        // 1,000,000 * 0.1% = 1,000
        let release_fee_amount = calculate_release_fee(amount, release_fee).unwrap();
        assert_eq!(release_fee_amount, Uint128::from(1_000u128));
    }
}
