#[cfg(test)]
#[allow(unused_imports)]
pub use inner::*;

#[cfg(test)]
mod inner {
    use crate::testing::helpers::{init, seed_chain_lp, seed_liquidity, seed_pool, MockDeps};
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::Uint256;
    use euclid::chain::ChainUid;
    use rstest::fixture;

    /// Fixture: VLP instantiated, no pools registered yet.
    #[fixture]
    pub fn initialized() -> MockDeps {
        let mut deps = mock_dependencies();
        init(&mut deps);
        deps
    }

    /// Fixture: VLP instantiated with one pool registered for "chain1".
    #[fixture]
    pub fn with_pool() -> MockDeps {
        let mut deps = initialized();
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_pool(&mut deps, &chain_uid);
        deps
    }

    /// Fixture: VLP instantiated with pool registered and liquidity seeded.
    /// Reserves: token1 = 1_000_000, token2 = 1_000_000, total_lp = 1_000_000.
    #[fixture]
    pub fn with_liquidity() -> MockDeps {
        let mut deps = with_pool();
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let total_lp = Uint256::from(1_000_000u128);
        seed_liquidity(
            &mut deps,
            Uint256::from(1_000_000u128),
            Uint256::from(1_000_000u128),
            total_lp,
        );
        // Give the chain a share of the LP tokens equal to total_lp (minus minimum liquidity)
        seed_chain_lp(&mut deps, &chain_uid, Uint256::from(999_000u128));
        deps
    }
}
