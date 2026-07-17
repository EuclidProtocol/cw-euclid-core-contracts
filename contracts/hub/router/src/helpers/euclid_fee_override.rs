use cosmwasm_std::Storage;
use euclid::{cross_chain_user::CrossChainUser, error::ContractError};

use crate::state::EUCLID_FEE_OVERRIDES;

/// Single resolution point for a wallet's Euclid-fee override. Returns
/// `Some(bps)` if a per-wallet override has been set for the
/// `(chain_uid, address)` of the swapping user, or `None` for the pool default.
///
/// Both the execute and simulate paths must go through this helper so quote
/// and execution can never drift.
pub fn get_euclid_fee_override(
    storage: &dyn Storage,
    user: &CrossChainUser,
) -> Result<Option<u64>, ContractError> {
    Ok(EUCLID_FEE_OVERRIDES.may_load(storage, (user.chain_uid.clone(), user.address.clone()))?)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_dependencies;
    use euclid::{chain::ChainUid, cross_chain_user::CrossChainUser};

    use super::*;

    fn user(chain: &str, address: &str) -> CrossChainUser {
        CrossChainUser::new(
            ChainUid::create(chain.to_string()).unwrap(),
            address.to_string(),
        )
    }

    #[test]
    fn absent_entry_returns_none() {
        let deps = mock_dependencies();
        let u = user("chain1", "wallet1");
        assert_eq!(
            get_euclid_fee_override(deps.as_ref().storage, &u).unwrap(),
            None
        );
    }

    #[test]
    fn present_entry_returns_stored_value() {
        let mut deps = mock_dependencies();
        let u = user("chain1", "wallet1");
        EUCLID_FEE_OVERRIDES
            .save(
                deps.as_mut().storage,
                (u.chain_uid.clone(), u.address.clone()),
                &25u64,
            )
            .unwrap();
        assert_eq!(
            get_euclid_fee_override(deps.as_ref().storage, &u).unwrap(),
            Some(25)
        );
    }

    #[test]
    fn zero_is_stored_distinct_from_absent() {
        let mut deps = mock_dependencies();
        let u = user("chain1", "wallet1");
        EUCLID_FEE_OVERRIDES
            .save(
                deps.as_mut().storage,
                (u.chain_uid.clone(), u.address.clone()),
                &0u64,
            )
            .unwrap();
        // Some(0) means "full exemption"; must not collapse to None.
        assert_eq!(
            get_euclid_fee_override(deps.as_ref().storage, &u).unwrap(),
            Some(0)
        );
    }

    #[test]
    fn same_address_on_different_chains_resolves_independently() {
        let mut deps = mock_dependencies();
        let u1 = user("chain1", "wallet");
        let u2 = user("chain2", "wallet");
        EUCLID_FEE_OVERRIDES
            .save(
                deps.as_mut().storage,
                (u1.chain_uid.clone(), u1.address.clone()),
                &10u64,
            )
            .unwrap();
        assert_eq!(
            get_euclid_fee_override(deps.as_ref().storage, &u1).unwrap(),
            Some(10)
        );
        assert_eq!(
            get_euclid_fee_override(deps.as_ref().storage, &u2).unwrap(),
            None
        );
    }

    #[test]
    fn removed_entry_returns_none() {
        let mut deps = mock_dependencies();
        let u = user("chain1", "wallet1");
        EUCLID_FEE_OVERRIDES
            .save(
                deps.as_mut().storage,
                (u.chain_uid.clone(), u.address.clone()),
                &50u64,
            )
            .unwrap();
        EUCLID_FEE_OVERRIDES.remove(
            deps.as_mut().storage,
            (u.chain_uid.clone(), u.address.clone()),
        );
        assert_eq!(
            get_euclid_fee_override(deps.as_ref().storage, &u).unwrap(),
            None
        );
    }
}
