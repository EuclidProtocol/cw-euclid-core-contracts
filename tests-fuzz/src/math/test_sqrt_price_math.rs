use concentrated_vlp::math::sqrt_price_math::{
    get_amount0_delta, get_amount1_delta, get_next_sqrt_price_from_input,
};
use concentrated_vlp::math::tick_math::get_sqrt_ratio_at_tick;
use cosmwasm_std::{Uint128, Uint256};
use proptest::prelude::*;

fn arb_tick() -> impl Strategy<Value = i64> {
    -50000i64..50000i64
}

fn arb_liquidity() -> impl Strategy<Value = Uint128> {
    (1_000u128..1_000_000_000u128).prop_map(Uint128::new)
}

fn arb_amount() -> impl Strategy<Value = Uint256> {
    (1u128..10_000_000u128).prop_map(Uint256::from)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]

    #[test]
    fn next_sqrt_price_from_input_direction_zero_for_one(
        tick in arb_tick(),
        liquidity in arb_liquidity(),
        amount_in in arb_amount(),
    ) {
        let sqrt_price = get_sqrt_ratio_at_tick(tick).unwrap();
        if let Ok(next) = get_next_sqrt_price_from_input(sqrt_price, liquidity, amount_in, true) {
            prop_assert!(next <= sqrt_price, "zero_for_one: next {} should <= current {}", next, sqrt_price);
        }
    }

    #[test]
    fn next_sqrt_price_from_input_direction_one_for_zero(
        tick in arb_tick(),
        liquidity in arb_liquidity(),
        amount_in in arb_amount(),
    ) {
        let sqrt_price = get_sqrt_ratio_at_tick(tick).unwrap();
        if let Ok(next) = get_next_sqrt_price_from_input(sqrt_price, liquidity, amount_in, false) {
            prop_assert!(next >= sqrt_price, "one_for_zero: next {} should >= current {}", next, sqrt_price);
        }
    }

    #[test]
    fn zero_amount_identity(
        tick in arb_tick(),
        liquidity in arb_liquidity(),
        zero_for_one in proptest::bool::ANY,
    ) {
        let sqrt_price = get_sqrt_ratio_at_tick(tick).unwrap();
        let result = get_next_sqrt_price_from_input(sqrt_price, liquidity, Uint256::zero(), zero_for_one);
        prop_assert!(result.is_ok());
        prop_assert_eq!(result.unwrap(), sqrt_price);
    }

    #[test]
    fn amount0_delta_symmetry(
        tick_a in arb_tick(),
        tick_b in arb_tick(),
        liquidity in arb_liquidity(),
    ) {
        let sqrt_a = get_sqrt_ratio_at_tick(tick_a).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(tick_b).unwrap();
        if sqrt_a != sqrt_b {
            let delta_ab = get_amount0_delta(sqrt_a, sqrt_b, liquidity, true);
            let delta_ba = get_amount0_delta(sqrt_b, sqrt_a, liquidity, true);
            if let (Ok(d1), Ok(d2)) = (delta_ab, delta_ba) {
                prop_assert_eq!(d1, d2, "amount0_delta should be symmetric");
            }
        }
    }

    #[test]
    fn amount1_delta_symmetry(
        tick_a in arb_tick(),
        tick_b in arb_tick(),
        liquidity in arb_liquidity(),
    ) {
        let sqrt_a = get_sqrt_ratio_at_tick(tick_a).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(tick_b).unwrap();
        if sqrt_a != sqrt_b {
            let delta_ab = get_amount1_delta(sqrt_a, sqrt_b, liquidity, false);
            let delta_ba = get_amount1_delta(sqrt_b, sqrt_a, liquidity, false);
            if let (Ok(d1), Ok(d2)) = (delta_ab, delta_ba) {
                prop_assert_eq!(d1, d2, "amount1_delta should be symmetric");
            }
        }
    }

    #[test]
    fn rounding_difference_bounded(
        tick_a in arb_tick(),
        tick_b in arb_tick(),
        liquidity in arb_liquidity(),
    ) {
        let sqrt_a = get_sqrt_ratio_at_tick(tick_a).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(tick_b).unwrap();
        if sqrt_a != sqrt_b {
            if let (Ok(rounded_down), Ok(rounded_up)) = (
                get_amount0_delta(sqrt_a, sqrt_b, liquidity, false),
                get_amount0_delta(sqrt_a, sqrt_b, liquidity, true),
            ) {
                let diff = rounded_up - rounded_down;
                prop_assert!(diff <= Uint256::from(1u128), "rounding diff for amount0 should be <= 1");
            }
        }
    }
}
