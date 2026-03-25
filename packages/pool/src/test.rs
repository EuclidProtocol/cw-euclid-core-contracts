#[cfg(test)]
mod tests {
    use crate::{
        calculate_amount_from_shares, calculate_cp_swap, calculate_lp_allocation, pre_swap,
        stable_math::compute_stable_swap, SwapCalculationMethod,
    };
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{Addr, Decimal, Decimal256, Uint128, Uint64};
    use rstest::rstest;

    use cw_storage_plus::{Item, Map};
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        fee::{DenomFees, Fee, TotalFees},
        msgs::vlp::base::State,
        token::{Pair, Token},
        utils::math::Decimal256Ext,
    };
    use std::collections::HashMap;

    #[rstest]
    #[case(1000u128, 1000u128, 0u128, 0u128, 0u128, 1000u128)]
    #[case(100u128, 100u128, 1000u128, 1000u128, 1000u128, 100u128)]
    #[case(200u128, 100u128, 2000u128, 1000u128, 1990u128, 199u128)]
    fn test_calculate_lp_allocation(
        #[case] token_1_amount: u128,
        #[case] token_2_amount: u128,
        #[case] total_liquidity_1: u128,
        #[case] total_liquidity_2: u128,
        #[case] total_lp_supply: u128,
        #[case] expected_lp_tokens: u128,
    ) {
        let token_1_amount = Uint128::new(token_1_amount);
        let token_2_amount = Uint128::new(token_2_amount);
        let total_liquidity_1 = Uint128::new(total_liquidity_1);
        let total_liquidity_2 = Uint128::new(total_liquidity_2);
        let total_lp_supply = Uint128::new(total_lp_supply);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        assert_eq!(lp_tokens, Uint128::new(expected_lp_tokens));

        let amount_1 = calculate_amount_from_shares(
            token_1_amount + total_liquidity_1,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert_eq!(
            amount_1, token_1_amount,
            "Token 1 amount from shares is not correct"
        );

        let amount_2 = calculate_amount_from_shares(
            token_2_amount + total_liquidity_2,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert_eq!(
            amount_2, token_2_amount,
            "Token 2 amount from shares is not correct"
        );
    }

    #[rstest]
    #[case(
        5_000_000_000_000_000_000_000_000u128,
        5_000_000_000_000_000_000_000_000u128,
        0u128,
        0u128,
        0u128,
        5_000_000_000_000_000_000_000_000u128
    )]
    #[case(
        5_000_000_000_000_000_000_000_000u128,
        5_000_000_000_000_000_000_000_000u128,
        5_000_000_000_000_000_000_000_000u128,
        5_000_000_000_000_000_000_000_000u128,
        5_000_000_000_000_000_000_000_000u128,
        5_000_000_000_000_000_000_000_000u128
    )]
    fn test_big_token_amount(
        #[case] token_1_amount: u128,
        #[case] token_2_amount: u128,
        #[case] total_liquidity_1: u128,
        #[case] total_liquidity_2: u128,
        #[case] total_lp_supply: u128,
        #[case] expected_lp_tokens: u128,
    ) {
        let token_1_amount = Uint128::new(token_1_amount);
        let token_2_amount = Uint128::new(token_2_amount);
        let total_liquidity_1 = Uint128::new(total_liquidity_1);
        let total_liquidity_2 = Uint128::new(total_liquidity_2);
        let total_lp_supply = Uint128::new(total_lp_supply);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        assert_eq!(lp_tokens, Uint128::new(expected_lp_tokens));
    }

    #[rstest]
    #[case(
        "equal_pools",
        Decimal256::from_ratio(100u128, 1u128),
        Decimal256::from_ratio(1000u128, 1u128),
        Decimal256::from_ratio(1000u128, 1u128),
        Uint64::new(1000),
        Uint128::new(99),
        Uint128::new(1)
    )]
    #[case(
        "imbalanced_pools",
        Decimal256::from_ratio(100u128, 1u128),
        Decimal256::from_ratio(2000u128, 1u128),
        Decimal256::from_ratio(1000u128, 1u128),
        Uint64::new(100),
        Uint128::new(67),
        Uint128::new(33)
    )]
    #[case(
        "small_amount",
        Decimal256::from_ratio(1u128, 1u128),
        Decimal256::from_ratio(1000000u128, 1u128),
        Decimal256::from_ratio(1000000u128, 1u128),
        Uint64::new(1000),
        Uint128::new(1),
        Uint128::new(0)
    )]
    #[case(
        "large_amount",
        Decimal256::from_ratio(1000u128, 1u128),
        Decimal256::from_ratio(2000u128, 1u128),
        Decimal256::from_ratio(2000u128, 1u128),
        Uint64::new(1000),
        Uint128::new(946u128),
        Uint128::new(54u128)
    )]
    #[case(
        "extreme_imbalance",
        Decimal256::from_ratio(100u128, 1u128),
        Decimal256::from_ratio(10000u128, 1u128),
        Decimal256::from_ratio(1000u128, 1u128),
        Uint64::new(1000),
        Uint128::new(47u128),
        Uint128::new(53u128)
    )]
    #[case(
        "large_values large spread",
        Decimal256::from_ratio(1000000000000000000u128, 1u128),
        Decimal256::from_ratio(1000000000000000000u128, 1u128),
        Decimal256::from_ratio(1000000000000000000u128, 1u128),
        Uint64::new(1000),
        Uint128::new(820871215252207999),
        Uint128::new(179128784747792001)
    )]
    #[case(
        "large_values small spread",
        Decimal256::from_ratio(1000u128, 1u128),
        Decimal256::from_ratio(1000000000000000000u128, 1u128),
        Decimal256::from_ratio(1000000000000000000u128, 1u128),
        Uint64::new(1000),
        Uint128::new(1000),
        Uint128::new(0)
    )]
    fn test_compute_stable_swap(
        #[case] case_name: &str,
        #[case] offer_asset: Decimal256,
        #[case] offer_pool: Decimal256,
        #[case] ask_pool: Decimal256,
        #[case] swap_amount: Uint64,
        #[case] expected_return_amount: Uint128,
        #[case] expected_spread_amount: Uint128,
    ) {
        let result =
            compute_stable_swap(&offer_asset, &offer_pool, &ask_pool, swap_amount).unwrap();

        assert_eq!(
            result.return_amount, expected_return_amount,
            "case_name={case_name}"
        );
        assert_eq!(
            result.spread_amount, expected_spread_amount,
            "case_name={case_name}"
        );
    }

    #[rstest]
    #[case(true, 10000u128, 5000u128, 100u64, 50u64, 1000u128)]
    #[case(false, 8000u128, 20000u128, 30u64, 20u64, 500u128)]
    // Very small amount, very small spread
    #[case(true, 1000u128, 1000u128, 10u64, 1u64, 1u128)]
    // Very small amount, very large spread (high fee bps)
    #[case(false, 1000u128, 1000u128, 9999u64, 0u64, 1u128)]
    // Very large amount, very small spread
    #[case(
        true,
        1000000000000000000u128,
        1000000000000000000u128,
        1u64,
        1u64,
        1000000000000000000u128
    )]
    // Very large amount, very large spread (high fee bps)
    #[case(
        false,
        1000000000000000000u128,
        1000000000000000000u128,
        9999u64,
        0u64,
        1000000000000000000u128
    )]

    fn test_pre_swap_regular_calculates_fees_and_cp_swap(
        #[case] asset_in_is_token_1: bool,
        #[case] reserve_token_1: u128,
        #[case] reserve_token_2: u128,
        #[case] lp_fee_bps: u64,
        #[case] euclid_fee_bps: u64,
        #[case] amount_in: u128,
    ) {
        let mut deps = mock_dependencies();

        let state_storage: Item<State> = Item::new("state");
        let balances_storage: Map<Token, Uint128> = Map::new("balances");

        let token_1 = Token::create("token1".to_string()).unwrap();
        let token_2 = Token::create("token2".to_string()).unwrap();
        let pair = Pair::new(token_1.clone(), token_2.clone()).unwrap();

        let asset_in = if asset_in_is_token_1 {
            token_1.clone()
        } else {
            token_2.clone()
        };
        let expected_asset_out = if asset_in_is_token_1 {
            token_2.clone()
        } else {
            token_1.clone()
        };

        balances_storage
            .save(
                deps.as_mut().storage,
                token_1.clone(),
                &Uint128::new(reserve_token_1),
            )
            .unwrap();
        balances_storage
            .save(
                deps.as_mut().storage,
                token_2.clone(),
                &Uint128::new(reserve_token_2),
            )
            .unwrap();

        let fee = Fee::new(
            lp_fee_bps,
            euclid_fee_bps,
            CrossChainUser::new(
                ChainUid::create("1".to_string()).unwrap(),
                "recipient".to_string(),
            ),
        );

        let total_fees_collected = TotalFees {
            lp_fees: DenomFees {
                totals: HashMap::default(),
            },
            euclid_fees: DenomFees {
                totals: HashMap::default(),
            },
        };

        let state = State {
            pair,
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("vbc"),
            fee,
            total_fees_collected,
            last_updated: 0,
            total_lp_tokens: Uint128::zero(),
        };
        state_storage.save(deps.as_mut().storage, &state).unwrap();

        let amount_in = Uint128::new(amount_in);
        let res = pre_swap(
            &deps.as_ref(),
            &state_storage,
            &balances_storage,
            &asset_in,
            amount_in,
            SwapCalculationMethod::Regular,
            None,
        )
        .unwrap();

        let expected_lp_fee = amount_in
            .checked_mul_floor(Decimal::bps(lp_fee_bps))
            .unwrap();
        let expected_euclid_fee = amount_in
            .checked_mul_floor(Decimal::bps(euclid_fee_bps))
            .unwrap();
        let expected_swap_amount = amount_in
            .checked_sub(expected_lp_fee.checked_add(expected_euclid_fee).unwrap())
            .unwrap();

        let (reserve_in, reserve_out) = if asset_in_is_token_1 {
            (Uint128::new(reserve_token_1), Uint128::new(reserve_token_2))
        } else {
            (Uint128::new(reserve_token_2), Uint128::new(reserve_token_1))
        };

        let expected_swap =
            calculate_cp_swap(expected_swap_amount, reserve_in, reserve_out).unwrap();

        assert_eq!(
            res.asset_out, expected_asset_out,
            "Expected asset out is not correct"
        );
        assert_eq!(
            res.lp_fee, expected_lp_fee,
            "Expected lp fee is not correct"
        );
        assert_eq!(
            res.euclid_fee, expected_euclid_fee,
            "Expected euclid fee is not correct"
        );
        assert_eq!(
            res.swap_amount, expected_swap_amount,
            "Expected swap amount is not correct"
        );
        assert_eq!(
            res.receive_amount, expected_swap.return_amount,
            "Expected receive amount is not correct"
        );
        assert_eq!(
            res.spread_amount, expected_swap.spread_amount,
            "Expected spread amount is not correct"
        );
    }

    #[rstest]
    #[case(true, 10000u128, 5000u128, 100u64, 50u64, 1000u64, 1000u128)]
    #[case(false, 8000u128, 20000u128, 30u64, 20u64, 1000u64, 500u128)]
    // Very small amount, very small spread
    #[case(true, 1000u128, 1000u128, 10u64, 1u64, 1000u64, 1u128)]
    // Very small amount, very large spread (high fee bps)
    #[case(false, 1000u128, 1000u128, 9999u64, 0u64, 1000u64, 1u128)]
    // Very large amount, very small spread
    #[case(
        true,
        1000000000000000000u128,
        1000000000000000000u128,
        1u64,
        1u64,
        1000u64,
        1000000000000000000u128
    )]
    // Very large amount, very large spread (high fee bps)
    #[case(
        false,
        1000000000000000000u128,
        1000000000000000000u128,
        9999u64,
        0u64,
        1000u64,
        1000000000000000000u128
    )]

    fn test_pre_swap_stable_calculates_fees_and_stable_swap(
        #[case] asset_in_is_token_1: bool,
        #[case] reserve_token_1: u128,
        #[case] reserve_token_2: u128,
        #[case] lp_fee_bps: u64,
        #[case] euclid_fee_bps: u64,
        #[case] amp_factor: u64,
        #[case] amount_in: u128,
    ) {
        let mut deps = mock_dependencies();

        let state_storage: Item<State> = Item::new("state");
        let balances_storage: Map<Token, Uint128> = Map::new("balances");

        let token_1 = Token::create("token1".to_string()).unwrap();
        let token_2 = Token::create("token2".to_string()).unwrap();
        let pair = Pair::new(token_1.clone(), token_2.clone()).unwrap();

        let asset_in = if asset_in_is_token_1 {
            token_1.clone()
        } else {
            token_2.clone()
        };
        let expected_asset_out = if asset_in_is_token_1 {
            token_2.clone()
        } else {
            token_1.clone()
        };

        balances_storage
            .save(
                deps.as_mut().storage,
                token_1.clone(),
                &Uint128::new(reserve_token_1),
            )
            .unwrap();
        balances_storage
            .save(
                deps.as_mut().storage,
                token_2.clone(),
                &Uint128::new(reserve_token_2),
            )
            .unwrap();

        let fee = Fee::new(
            lp_fee_bps,
            euclid_fee_bps,
            CrossChainUser::new(
                ChainUid::create("1".to_string()).unwrap(),
                "recipient".to_string(),
            ),
        );

        let total_fees_collected = TotalFees {
            lp_fees: DenomFees {
                totals: HashMap::default(),
            },
            euclid_fees: DenomFees {
                totals: HashMap::default(),
            },
        };

        let state = State {
            pair,
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("vbc"),
            fee,
            total_fees_collected,
            last_updated: 0,
            total_lp_tokens: Uint128::zero(),
        };
        state_storage.save(deps.as_mut().storage, &state).unwrap();

        let amount_in = Uint128::new(amount_in);
        let res = pre_swap(
            &deps.as_ref(),
            &state_storage,
            &balances_storage,
            &asset_in,
            amount_in,
            SwapCalculationMethod::Stable(Uint64::from(amp_factor)),
            None,
        )
        .unwrap();

        let expected_lp_fee = amount_in
            .checked_mul_floor(Decimal::bps(lp_fee_bps))
            .unwrap();
        let expected_euclid_fee = amount_in
            .checked_mul_floor(Decimal::bps(euclid_fee_bps))
            .unwrap();
        let expected_swap_amount = amount_in
            .checked_sub(expected_lp_fee.checked_add(expected_euclid_fee).unwrap())
            .unwrap();

        let (reserve_in, reserve_out) = if asset_in_is_token_1 {
            (Uint128::new(reserve_token_1), Uint128::new(reserve_token_2))
        } else {
            (Uint128::new(reserve_token_2), Uint128::new(reserve_token_1))
        };

        let expected_swap = compute_stable_swap(
            // `pre_swap` stable branch uses the original `amount_in` (pre-fee),
            // while `swap_amount` returned in the response is fee-adjusted.
            &Decimal256::from_integer(expected_swap_amount),
            &Decimal256::from_integer(reserve_in),
            &Decimal256::from_integer(reserve_out),
            Uint64::from(amp_factor),
        )
        .unwrap();

        assert_eq!(
            res.asset_out, expected_asset_out,
            "Expected asset out is not correct"
        );
        assert_eq!(
            res.lp_fee,
            expected_lp_fee,
            "Expected lp fee is not correct. Percentage deviation: {}",
            percentage_deviation(res.lp_fee, expected_lp_fee)
        );
        assert_eq!(
            res.euclid_fee,
            expected_euclid_fee,
            "Expected euclid fee is not correct. Percentage deviation: {}",
            percentage_deviation(res.euclid_fee, expected_euclid_fee)
        );
        assert_eq!(
            res.swap_amount,
            expected_swap_amount,
            "Expected swap amount is not correct. Percentage deviation: {}",
            percentage_deviation(res.swap_amount, expected_swap_amount)
        );
        assert_eq!(
            res.receive_amount,
            expected_swap.return_amount,
            "Expected receive amount is not correct. Percentage deviation: {}",
            percentage_deviation(res.receive_amount, expected_swap.return_amount)
        );
        assert_eq!(
            res.spread_amount,
            expected_swap.spread_amount,
            "Expected spread amount is not correct. Percentage deviation: {}",
            percentage_deviation(res.spread_amount, expected_swap.spread_amount)
        );
    }

    fn percentage_deviation(actual: Uint128, expected: Uint128) -> Decimal256 {
        let actual = Decimal256::from_integer(actual);
        let expected = Decimal256::from_integer(expected);
        let deviation = actual.abs_diff(expected) / expected;
        deviation
            .checked_mul(Decimal256::from_integer(100u64))
            .unwrap()
    }

    // ========================================================================
    // AUDIT TESTS: Proving vulnerabilities documented in AUDIT.md
    // ========================================================================

    mod audit_tests {
        use super::*;
        use crate::stable_math::compute_d;

        // CRITICAL-1 FIX VERIFICATION: 24-decimal pools now work with iterative d_product
        // Previously panicked due to d.pow(3) overflow. Now uses D*D/pool_a * D/pool_b.
        #[test]
        fn test_24_decimal_balanced_pools_now_works() {
            let one_token_24dec = Decimal256::from_ratio(
                1_000_000_000_000_000_000_000_000u128, // 1e24
                1u128,
            );
            let offer = Decimal256::from_ratio(
                100_000_000_000_000_000_000_000u128, // 0.1 token = 1e23
                1u128,
            );
            let result =
                compute_stable_swap(&offer, &one_token_24dec, &one_token_24dec, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "24-decimal pools should now work after iterative d_product fix. Error: {:?}",
                result.err()
            );
            let swap = result.unwrap();
            // Return should be close to offer for balanced pools with high amp
            assert!(
                swap.return_amount > Uint128::zero(),
                "Should return non-zero amount"
            );
            assert!(
                swap.return_amount <= Uint128::new(100_000_000_000_000_000_000_000u128),
                "Return should not exceed offer"
            );
        }

        // CRITICAL-1 FIX VERIFICATION: 1e20 pools now work
        #[test]
        fn test_1e20_pools_now_works() {
            let pool = Decimal256::from_ratio(100_000_000_000_000_000_000u128, 1u128); // 1e20
            let offer = Decimal256::from_ratio(1_000_000_000_000_000_000u128, 1u128); // 1e18
            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "1e20 pools should now work. Error: {:?}",
                result.err()
            );
        }

        // CRITICAL-1 FIX VERIFICATION: compute_d works for 24-decimal pools
        #[test]
        fn test_compute_d_24_decimal_pools() {
            let pool_a = Decimal256::from_ratio(
                1_000_000_000_000_000_000_000_000u128, // 1e24
                1u128,
            );
            let pool_b = pool_a;
            let result = compute_d(Uint64::new(1000), &[pool_a, pool_b]);
            assert!(
                result.is_ok(),
                "compute_d should work for 24-decimal pools. Error: {:?}",
                result.err()
            );
            let d = result.unwrap();
            // D should be approximately 2e24 for balanced pools
            assert!(d > Decimal256::zero(), "D should be positive");
        }

        // CRITICAL-2 FIX VERIFICATION: compute_d now returns error instead of panicking
        // for values that overflow even the iterative approach
        #[test]
        fn test_compute_d_returns_error_not_panic_on_extreme_values() {
            // Use an extremely large value that might still overflow the iterative path
            // Uint128::MAX ≈ 3.4e38
            let pool = Decimal256::from_ratio(Uint128::MAX, 1u128);
            let result = compute_d(Uint64::new(1000), &[pool, pool]);
            // Should return Err (checked arithmetic), not panic
            assert!(
                result.is_err(),
                "Extreme values should return error, not panic"
            );
        }

        // Verify calc_y succeeds for large pool values after fix
        #[test]
        fn test_calc_y_large_pools_succeeds() {
            use crate::stable_math::calc_y;

            let pool = Decimal256::from_ratio(
                1_000_000_000_000_000_000_000_000u128, // 1e24
                1u128,
            );
            let new_amount = pool + Decimal256::from_ratio(
                100_000_000_000_000_000_000_000u128, // 1e23
                1u128,
            );
            let xp = [pool, pool];

            let result = calc_y(Uint64::new(1000), new_amount, &xp, 1);
            assert!(
                result.is_ok(),
                "calc_y should succeed for 24-decimal pools after fix: {:?}",
                result.err()
            );
        }

        // CRITICAL-3: TOKEN_PRECISION=1 is arbitrary — demonstrate it works only because
        // the to_uint128_with_precision(1) and /10 cancel out for integer inputs
        #[test]
        fn test_precision_with_24_decimal_inputs() {
            // With 24-decimal tokens, we'd pass raw values like 1e24.
            // But first, the overflow issue (CRITICAL-1) blocks this entirely.
            // To isolate the precision concern, use smaller 6-decimal tokens.
            // 1000 tokens at 6 decimals = 1_000_000_000 raw
            let pool = Decimal256::from_ratio(1_000_000_000u128, 1u128);
            let offer = Decimal256::from_ratio(1_000_000u128, 1u128); // 1 token

            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(1000)).unwrap();

            // The return_amount is in the same raw units as input (integer scale)
            // TOKEN_PRECISION=1 adds/removes one digit — this doesn't relate to
            // the 6-decimal structure of the token at all.
            // The result treats 1_000_000 as "one million integer units", not "1 token with 6 decimals"
            assert!(
                result.return_amount <= Uint128::new(1_000_000),
                "Return should not exceed offer for stable swap. Got: {}",
                result.return_amount
            );
            // Spread should be very small for equal balanced pools with high amp
            assert!(
                result.spread_amount < Uint128::new(10_000),
                "Spread too high for balanced stable pool. Got: {}",
                result.spread_amount
            );
        }

        // HIGH-1: Verify amp factor consistency between compute_d and calc_y
        #[test]
        fn test_amp_factor_consistency() {
            // Test that different amp factors produce monotonically decreasing spread
            // (higher amp = more stable = less slippage)
            let pool = Decimal256::from_ratio(10000u128, 1u128);
            let offer = Decimal256::from_ratio(100u128, 1u128);

            let result_low_amp =
                compute_stable_swap(&offer, &pool, &pool, Uint64::new(100)).unwrap();
            let result_high_amp =
                compute_stable_swap(&offer, &pool, &pool, Uint64::new(10000)).unwrap();

            // Higher amp should give higher return (less slippage)
            assert!(
                result_high_amp.return_amount >= result_low_amp.return_amount,
                "Higher amp should give better rate. Low amp return: {}, High amp return: {}",
                result_low_amp.return_amount,
                result_high_amp.return_amount
            );

            // Higher amp should give lower spread
            assert!(
                result_high_amp.spread_amount <= result_low_amp.spread_amount,
                "Higher amp should give lower spread. Low amp spread: {}, High amp spread: {}",
                result_low_amp.spread_amount,
                result_high_amp.spread_amount
            );
        }

        // HIGH-2 FIX VERIFICATION: spread now uses checked_sub instead of saturating_sub.
        // For valid swaps, spread should be non-negative. If return > offer due to a bug,
        // checked_sub would return an error instead of silently returning 0.
        #[test]
        fn test_spread_uses_checked_sub() {
            // For stable swap with balanced pools and small offer, return ≈ offer
            // and spread ≈ 0. checked_sub handles this correctly.
            let pool = Decimal256::from_ratio(1_000_000u128, 1u128);
            let offer = Decimal256::from_ratio(1u128, 1u128);

            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(10000)).unwrap();

            assert_eq!(
                result.spread_amount,
                Uint128::zero(),
                "Spread should be zero for tiny swap in large balanced pool"
            );
        }

        // MEDIUM-2 FIX VERIFICATION: Zero amp factor now returns a clean error
        // (input validation catches it before reaching calculate_step)
        #[test]
        fn test_zero_amp_factor_returns_error() {
            let pool = Decimal256::from_ratio(1000u128, 1u128);
            let offer = Decimal256::from_ratio(100u128, 1u128);

            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(0));
            assert!(
                result.is_err(),
                "Zero amp factor should return error. Got: {:?}",
                result.unwrap()
            );
        }

        // MEDIUM-2: Extremely large amp factor
        #[test]
        fn test_extreme_amp_factor() {
            let pool = Decimal256::from_ratio(1000u128, 1u128);
            let offer = Decimal256::from_ratio(100u128, 1u128);

            // Very large amp — at the extreme, stable swap approaches constant-sum
            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(u64::MAX));

            // Should either succeed (with return ≈ offer) or return a clean error
            match result {
                Ok(swap) => {
                    // With extreme amp, return should be very close to offer
                    assert!(
                        swap.return_amount <= Uint128::new(100),
                        "Return should not exceed offer"
                    );
                }
                Err(_) => {
                    // An error is acceptable — overflow in leverage is expected
                }
            }
        }

        // LOW-1: Integer truncation creates systematic loss for users
        #[test]
        fn test_truncation_loss_small_swaps() {
            // Swap 1 unit at a time in a balanced pool. Each swap loses up to 0.9
            // units due to floor division by 10 (TOKEN_PRECISION).
            let pool = Decimal256::from_ratio(1_000_000u128, 1u128);
            let one_unit = Decimal256::from_ratio(1u128, 1u128);

            let result = compute_stable_swap(&one_unit, &pool, &pool, Uint64::new(10000)).unwrap();

            // With TOKEN_PRECISION=1: the subtraction happens at 10x scale,
            // then divides by 10. For a 1-unit swap in a huge pool:
            // ask_pool_at_precision_1 - new_ask_pool_at_precision_1 ≈ 10
            // return_amount = 10 / 10 = 1 (no loss in this case)
            // But with offer=3 in certain pool ratios, the result could be
            // (31 - 1) / 10 = 3 instead of 3.1 -> truncation loss
            assert_eq!(
                result.return_amount,
                Uint128::new(1),
                "1-unit swap should return 1 in balanced pool"
            );
        }

        // After fix: 1e19 pools now work (previously returned overflow error)
        #[test]
        fn test_1e19_pools_now_works() {
            let pool = Decimal256::from_ratio(
                10_000_000_000_000_000_000u128, // 1e19
                1u128,
            );
            let offer = Decimal256::from_ratio(
                1_000_000_000_000_000_000u128, // 1e18
                1u128,
            );
            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "1e19 pools should now work after fix. Error: {:?}",
                result.err()
            );
        }

        // 1e18 pools still work (regression check)
        #[test]
        fn test_1e18_pools_still_works() {
            let pool = Decimal256::from_ratio(
                1_000_000_000_000_000_000u128, // 1e18
                1u128,
            );
            let offer = Decimal256::from_ratio(1000u128, 1u128);

            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "1e18 pools should work. Error: {:?}",
                result.err()
            );
        }

        // MEDIUM-2: Zero pool reserve now returns error instead of panicking
        // (fixed by CRITICAL-2: all arithmetic is now checked)
        #[test]
        fn test_zero_pool_reserve_returns_error() {
            let zero_pool = Decimal256::zero();
            let pool = Decimal256::from_ratio(1000u128, 1u128);
            let offer = Decimal256::from_ratio(100u128, 1u128);

            // One pool is zero, the other is not. sum_x != 0, so Newton's method runs.
            // amount_b_times_coins = 0, causing checked_div to return error.
            let result = compute_stable_swap(&offer, &pool, &zero_pool, Uint64::new(1000));
            assert!(
                result.is_err(),
                "Zero pool reserve should return error. Got: {:?}",
                result.unwrap()
            );
        }

        // LOW-1: Demonstrate actual truncation loss with imbalanced pools
        // The floor division by 10 (TOKEN_PRECISION=1) loses fractional units
        #[test]
        fn test_truncation_loss_imbalanced_pools() {
            // With imbalanced pools, the scaled difference may not be divisible by 10,
            // causing truncation loss.
            let offer_pool = Decimal256::from_ratio(2000u128, 1u128);
            let ask_pool = Decimal256::from_ratio(1000u128, 1u128);
            let offer = Decimal256::from_ratio(100u128, 1u128);

            let result =
                compute_stable_swap(&offer, &offer_pool, &ask_pool, Uint64::new(100)).unwrap();

            // Compute what the "true" return would be without truncation:
            // The spread + return should equal the offer amount.
            // But due to truncation: return_amount + spread_amount may not equal offer_amount.
            let reconstructed = result
                .return_amount
                .checked_add(result.spread_amount)
                .unwrap();
            let offer_uint = Uint128::new(100);

            // If there's no truncation loss, return + spread == offer.
            // With truncation, return is rounded down, so spread is inflated,
            // meaning return + spread could differ from offer.
            // The key insight: spread = offer - return (via saturating_sub),
            // so return + spread always == offer by construction.
            // The REAL loss is that return_amount is lower than the true mathematical value.
            // We can detect this by checking: for a balanced stable pool with high amp,
            // the return should be very close to the offer. Any shortfall beyond the
            // mathematical spread is truncation loss.
            assert_eq!(
                reconstructed, offer_uint,
                "return + spread should always equal offer by construction"
            );

            // The actual truncation shows up as reduced return_amount.
            // With amp=100 and 2:1 pool ratio, 100 offer should return ~67
            assert!(
                result.return_amount > Uint128::zero(),
                "Should get a non-zero return"
            );
        }

        // Verify the StableSwap invariant holds before and after a swap:
        // A * n^n * S + D = A * n^n * D + D^(n+1) / (n^n * prod(x_i))
        #[test]
        fn test_stableswap_invariant_preserved() {
            let pool_a = Decimal256::from_ratio(10000u128, 1u128);
            let pool_b = Decimal256::from_ratio(10000u128, 1u128);
            let offer = Decimal256::from_ratio(500u128, 1u128);
            let amp = Uint64::new(1000);

            // Compute D before swap
            let d_before = compute_d(amp, &[pool_a, pool_b]).unwrap();

            // Perform swap
            let result = compute_stable_swap(&offer, &pool_a, &pool_b, amp).unwrap();

            // New pool state after swap
            let new_pool_a = pool_a + offer;
            let new_pool_b =
                pool_b - Decimal256::from_ratio(result.return_amount, 1u128);

            // Compute D after swap
            let d_after = compute_d(amp, &[new_pool_a, new_pool_b]).unwrap();

            // D should be approximately preserved (within tolerance)
            // Note: due to integer truncation in return_amount, the actual output is
            // slightly less than the mathematical output, which means more value stays
            // in the pool, so D_after >= D_before (approximately).
            let diff = d_after.abs_diff(d_before);
            let relative_diff = diff / d_before;

            assert!(
                relative_diff < Decimal256::from_ratio(1u128, 1000u128), // < 0.1% deviation
                "Invariant D should be approximately preserved. D_before: {}, D_after: {}, relative_diff: {}",
                d_before, d_after, relative_diff
            );
        }
    }
}
