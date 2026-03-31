use concentrated_vlp::math::swap_math::compute_swap_step_exact_input;
use concentrated_vlp::math::tick_math::get_sqrt_ratio_at_tick;
use cosmwasm_std::{Uint128, Uint256};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]

    #[test]
    fn price_monotonicity_zero_for_one(
        current_tick in -50000i64..50000i64,
        target_offset in 1i64..1000i64,
        liquidity in 1_000u128..1_000_000_000u128,
        amount in 100u128..10_000_000u128,
        fee_pips in 1u64..100_000u64,
    ) {
        let target_tick = current_tick - target_offset;
        if target_tick < -887272 { return Ok(()); }

        let sqrt_current = get_sqrt_ratio_at_tick(current_tick).unwrap();
        let sqrt_target = get_sqrt_ratio_at_tick(target_tick).unwrap();

        if let Ok(step) = compute_swap_step_exact_input(
            sqrt_current,
            sqrt_target,
            Uint128::new(liquidity),
            Uint256::from(amount),
            fee_pips,
        ) {
            prop_assert!(
                step.sqrt_ratio_next_x96 >= sqrt_target && step.sqrt_ratio_next_x96 <= sqrt_current,
                "zero_for_one: next price {} should be between target {} and current {}",
                step.sqrt_ratio_next_x96, sqrt_target, sqrt_current
            );
        }
    }

    #[test]
    fn price_monotonicity_one_for_zero(
        current_tick in -50000i64..50000i64,
        target_offset in 1i64..1000i64,
        liquidity in 1_000u128..1_000_000_000u128,
        amount in 100u128..10_000_000u128,
        fee_pips in 1u64..100_000u64,
    ) {
        let target_tick = current_tick + target_offset;
        if target_tick > 887272 { return Ok(()); }

        let sqrt_current = get_sqrt_ratio_at_tick(current_tick).unwrap();
        let sqrt_target = get_sqrt_ratio_at_tick(target_tick).unwrap();

        if let Ok(step) = compute_swap_step_exact_input(
            sqrt_current,
            sqrt_target,
            Uint128::new(liquidity),
            Uint256::from(amount),
            fee_pips,
        ) {
            prop_assert!(
                step.sqrt_ratio_next_x96 >= sqrt_current && step.sqrt_ratio_next_x96 <= sqrt_target,
                "one_for_zero: next price {} should be between current {} and target {}",
                step.sqrt_ratio_next_x96, sqrt_current, sqrt_target
            );
        }
    }

    #[test]
    fn consumed_bounded(
        current_tick in -50000i64..50000i64,
        target_offset in 1i64..1000i64,
        liquidity in 1_000u128..1_000_000_000u128,
        amount in 100u128..10_000_000u128,
        fee_pips in 1u64..100_000u64,
    ) {
        let target_tick = current_tick - target_offset;
        if target_tick < -887272 { return Ok(()); }

        let sqrt_current = get_sqrt_ratio_at_tick(current_tick).unwrap();
        let sqrt_target = get_sqrt_ratio_at_tick(target_tick).unwrap();
        let amount_remaining = Uint256::from(amount);

        if let Ok(step) = compute_swap_step_exact_input(
            sqrt_current,
            sqrt_target,
            Uint128::new(liquidity),
            amount_remaining,
            fee_pips,
        ) {
            let consumed = step.amount_in + step.fee_amount;
            prop_assert!(
                consumed <= amount_remaining,
                "consumed {} should <= remaining {}",
                consumed, amount_remaining
            );
        }
    }

    #[test]
    fn identity_swap(
        tick in -50000i64..50000i64,
        liquidity in 1_000u128..1_000_000_000u128,
        fee_pips in 1u64..100_000u64,
    ) {
        let sqrt_price = get_sqrt_ratio_at_tick(tick).unwrap();
        // When current == target, everything should be zero
        if let Ok(step) = compute_swap_step_exact_input(
            sqrt_price,
            sqrt_price,
            Uint128::new(liquidity),
            Uint256::from(1000u128),
            fee_pips,
        ) {
            prop_assert_eq!(step.amount_in, Uint256::zero());
            prop_assert_eq!(step.amount_out, Uint256::zero());
            prop_assert_eq!(step.fee_amount, Uint256::zero());
            prop_assert_eq!(step.sqrt_ratio_next_x96, sqrt_price);
        }
    }
}
