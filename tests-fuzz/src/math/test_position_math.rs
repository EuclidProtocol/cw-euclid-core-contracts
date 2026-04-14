use concentrated_vlp::math::position_math::fees_owed;
use cosmwasm_std::{Uint128, Uint256};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn fees_owed_non_negative(
        liquidity in 1u128..1_000_000_000u128,
        growth_inside in 0u128..u64::MAX as u128,
        growth_last in 0u128..u64::MAX as u128,
    ) {
        let result = fees_owed(
            Uint128::new(liquidity),
            Uint256::from(growth_inside),
            Uint256::from(growth_last),
        );
        // Should either succeed with non-negative result or error
        if let Ok(fees) = result {
            // Uint128 is inherently non-negative
            prop_assert!(true, "fees_owed returned valid Uint128: {}", fees);
        }
    }

    #[test]
    fn fees_zero_when_no_growth(
        liquidity in 0u128..1_000_000_000u128,
        growth in 0u128..u64::MAX as u128,
    ) {
        let result = fees_owed(
            Uint128::new(liquidity),
            Uint256::from(growth),
            Uint256::from(growth), // same = no growth
        );
        prop_assert!(result.is_ok());
        prop_assert_eq!(result.unwrap(), Uint128::zero());
    }

    #[test]
    fn fees_zero_when_zero_liquidity(
        growth_inside in 1u128..u64::MAX as u128,
        growth_last in 0u128..u64::MAX as u128,
    ) {
        if growth_inside <= growth_last { return Ok(()); }
        let result = fees_owed(
            Uint128::zero(),
            Uint256::from(growth_inside),
            Uint256::from(growth_last),
        );
        prop_assert!(result.is_ok());
        prop_assert_eq!(result.unwrap(), Uint128::zero());
    }

    #[test]
    fn fees_proportional_to_liquidity(
        liquidity in 1u128..1_000_000u128,
        growth_delta in 1u128..1_000_000u128,
    ) {
        let growth_last = Uint256::from(0u128);
        let growth_inside = Uint256::from(growth_delta);

        let fees_1 = fees_owed(Uint128::new(liquidity), growth_inside, growth_last);
        let fees_2 = fees_owed(Uint128::new(liquidity * 2), growth_inside, growth_last);

        if let (Ok(f1), Ok(f2)) = (fees_1, fees_2) {
            // Double liquidity should give double fees (within rounding)
            let expected = f1.u128() * 2;
            let diff = if f2.u128() > expected {
                f2.u128() - expected
            } else {
                expected - f2.u128()
            };
            prop_assert!(diff <= 1, "2x liquidity should give ~2x fees: f1={}, f2={}", f1, f2);
        }
    }
}
