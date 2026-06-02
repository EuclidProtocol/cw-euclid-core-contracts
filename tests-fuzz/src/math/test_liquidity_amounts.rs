use concentrated_vlp::math::liquidity_amounts::{
    get_amounts_for_liquidity, get_liquidity_for_amount0, get_liquidity_for_amounts,
};
use concentrated_vlp::math::tick_math::get_sqrt_ratio_at_tick;
use cosmwasm_std::{Uint128, Uint256};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]

    #[test]
    fn below_range_uses_token0_only(
        lower_tick in -50000i64..-100i64,
        range_size in 100i64..5000i64,
        amount0 in 100u128..1_000_000u128,
        amount1 in 100u128..1_000_000u128,
    ) {
        let upper_tick = lower_tick + range_size;
        if upper_tick > 887272 { return Ok(()); }

        let sqrt_a = get_sqrt_ratio_at_tick(lower_tick).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(upper_tick).unwrap();
        // Current price below range
        let current_tick = lower_tick - 1000;
        if current_tick < -887272 { return Ok(()); }
        let sqrt_p = get_sqrt_ratio_at_tick(current_tick).unwrap();

        let liquidity = get_liquidity_for_amounts(
            sqrt_p, sqrt_a, sqrt_b,
            Uint128::new(amount0), Uint128::new(amount1),
        );
        if let Ok(l) = liquidity {
            if !l.is_zero() {
                let (a0, a1) = get_amounts_for_liquidity(sqrt_p, sqrt_a, sqrt_b, l, false).unwrap();
                prop_assert!(a0 > Uint256::zero() || l.is_zero(), "below range should use token0");
                prop_assert_eq!(a1, Uint256::zero(), "below range should not use token1");
            }
        }
    }

    #[test]
    fn above_range_uses_token1_only(
        lower_tick in -50000i64..0i64,
        range_size in 100i64..5000i64,
        amount0 in 100u128..1_000_000u128,
        amount1 in 100u128..1_000_000u128,
    ) {
        let upper_tick = lower_tick + range_size;
        if upper_tick > 887272 { return Ok(()); }

        let sqrt_a = get_sqrt_ratio_at_tick(lower_tick).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(upper_tick).unwrap();
        // Current price above range
        let current_tick = upper_tick + 1000;
        if current_tick > 887272 { return Ok(()); }
        let sqrt_p = get_sqrt_ratio_at_tick(current_tick).unwrap();

        let liquidity = get_liquidity_for_amounts(
            sqrt_p, sqrt_a, sqrt_b,
            Uint128::new(amount0), Uint128::new(amount1),
        );
        if let Ok(l) = liquidity {
            if !l.is_zero() {
                let (a0, a1) = get_amounts_for_liquidity(sqrt_p, sqrt_a, sqrt_b, l, false).unwrap();
                prop_assert_eq!(a0, Uint256::zero(), "above range should not use token0");
                prop_assert!(a1 > Uint256::zero() || l.is_zero(), "above range should use token1");
            }
        }
    }

    #[test]
    fn zero_amounts_give_zero_liquidity(
        lower_tick in -50000i64..0i64,
        range_size in 100i64..5000i64,
    ) {
        let upper_tick = lower_tick + range_size;
        if upper_tick > 887272 { return Ok(()); }

        let sqrt_a = get_sqrt_ratio_at_tick(lower_tick).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(upper_tick).unwrap();
        let sqrt_p = get_sqrt_ratio_at_tick(0).unwrap();

        let liquidity = get_liquidity_for_amounts(
            sqrt_p, sqrt_a, sqrt_b,
            Uint128::zero(), Uint128::zero(),
        );
        if let Ok(l) = liquidity {
            prop_assert!(l.is_zero(), "zero amounts should give zero liquidity");
        }
    }

    #[test]
    fn roundtrip_below_range(
        lower_tick in -50000i64..-100i64,
        range_size in 100i64..5000i64,
        amount0 in 1000u128..100_000u128,
    ) {
        let upper_tick = lower_tick + range_size;
        if upper_tick > 887272 { return Ok(()); }

        let sqrt_a = get_sqrt_ratio_at_tick(lower_tick).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(upper_tick).unwrap();

        let liquidity = get_liquidity_for_amount0(sqrt_a, sqrt_b, Uint128::new(amount0));
        if let Ok(l) = liquidity {
            if !l.is_zero() {
                // Current below range
                let below_tick = (lower_tick - 1000).max(-887272);
                let sqrt_p = get_sqrt_ratio_at_tick(below_tick).unwrap();
                let (recovered_0, _) = get_amounts_for_liquidity(sqrt_p, sqrt_a, sqrt_b, l, false).unwrap();
                let recovered = Uint128::try_from(recovered_0).unwrap_or(Uint128::MAX);
                // Should be close (within rounding)
                let diff = if recovered > Uint128::new(amount0) {
                    recovered.u128() - amount0
                } else {
                    amount0 - recovered.u128()
                };
                // Rounding down in both get_liquidity_for_amount0 and get_amounts_for_liquidity
                // means recovered <= amount0 (no value creation). The absolute error
                // grows with tick range width due to fixed-point precision limits.
                prop_assert!(recovered <= Uint128::new(amount0),
                    "roundtrip should not create value: recovered {} > original {}", recovered, amount0);
                // Relative tolerance: error should be < 0.1% of input
                let max_diff = (amount0 / 1000).max(5);
                prop_assert!(diff <= max_diff,
                    "roundtrip diff {} exceeds 0.1% tolerance {} for amount {}", diff, max_diff, amount0);
            }
        }
    }
}
