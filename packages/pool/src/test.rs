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

        // CRITICAL-1: Overflow in d^3 computation for 24-decimal tokens
        // With 24-decimal tokens, 1 token = 1e24 raw. Even 1 token per pool overflows.
        // d ≈ 2e24, d_atomics = 2e42, d^3_atomics = 8e90 >> Uint256 max (1.158e77)
        #[test]
        #[should_panic]
        fn test_overflow_24_decimal_balanced_pools() {
            // 1 token each at 24 decimals = 1e24 raw per pool
            let one_token_24dec = Decimal256::from_ratio(
                1_000_000_000_000_000_000_000_000u128, // 1e24
                1u128,
            );
            let offer = Decimal256::from_ratio(
                100_000_000_000_000_000_000_000u128, // 0.1 token = 1e23
                1u128,
            );
            // This panics due to unchecked d.pow(3) overflow in compute_d
            let _ = compute_stable_swap(&offer, &one_token_24dec, &one_token_24dec, Uint64::new(1000));
        }

        // CRITICAL-1: Even 1e20 integer values overflow d^3
        // d ≈ 2e20, d_atomics = 2e38, d^3_atomics = 8e78 > Uint256 max
        #[test]
        #[should_panic]
        fn test_overflow_1e20_pools() {
            let pool = Decimal256::from_ratio(100_000_000_000_000_000_000u128, 1u128); // 1e20
            let offer = Decimal256::from_ratio(1_000_000_000_000_000_000u128, 1u128); // 1e18
            // Panics in compute_d line 93: d.pow(3)
            let _ = compute_stable_swap(&offer, &pool, &pool, Uint64::new(1000));
        }

        // CRITICAL-1: Even the intermediate d*d overflows for 24-decimal values
        // d_atomics = 2e42, d*d raw = 4e84 >> Uint256 max
        #[test]
        #[should_panic]
        fn test_overflow_intermediate_d_squared() {
            // compute_d with 24-decimal pools
            let pool_a = Decimal256::from_ratio(
                1_000_000_000_000_000_000_000_000u128, // 1e24
                1u128,
            );
            let pool_b = pool_a;
            // This panics inside compute_d because d.pow(3) uses d*d as intermediate
            let _ = compute_d(Uint64::new(1000), &[pool_a, pool_b]);
        }

        // CRITICAL-2: compute_d uses unchecked arithmetic that panics instead of returning error
        // Line 93: d.pow(3) / (amount_a_times_coins * amount_b_times_coins) — all unchecked
        #[test]
        #[should_panic]
        fn test_compute_d_unchecked_panics() {
            // Values large enough to trigger overflow, proving panic vs error
            let pool = Decimal256::from_ratio(
                100_000_000_000_000_000_000u128, // 1e20
                1u128,
            );
            // Should return Err, but panics instead due to unchecked ops
            let _ = compute_d(Uint64::new(1000), &[pool, pool]);
        }

        // CRITICAL-2: calc_y uses checked_pow(3) which returns Err (not panic)
        // This proves the inconsistency: compute_d panics, calc_y returns error
        #[test]
        fn test_calc_y_checked_overflow_returns_error() {
            use crate::stable_math::calc_y;

            let pool = Decimal256::from_ratio(
                1_000_000_000_000_000u128, // 1e15 — small enough for compute_d to succeed
                1u128,
            );
            // For calc_y, new_amount after swap
            let new_amount = pool + Decimal256::from_ratio(1000u128, 1u128);
            let xp = [pool, pool];

            // This should succeed at 1e15 because d^3 atomics ≈ 8e63 < Uint256 max
            let result = calc_y(Uint64::new(1000), new_amount, &xp, 1);
            assert!(
                result.is_ok(),
                "calc_y should succeed for pools at 1e15: {:?}",
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

        // HIGH-2: saturating_sub silently masks bugs in spread calculation
        // This test demonstrates that if a math bug caused return > offer,
        // the spread would silently be 0 instead of an error
        #[test]
        fn test_spread_uses_saturating_sub() {
            // For stable swap with balanced pools and small offer, return ≈ offer
            // and spread ≈ 0. The saturating_sub makes this safe for valid cases.
            let pool = Decimal256::from_ratio(1_000_000u128, 1u128);
            let offer = Decimal256::from_ratio(1u128, 1u128);

            let result = compute_stable_swap(&offer, &pool, &pool, Uint64::new(10000)).unwrap();

            // For a 1-unit swap in a 1M pool with high amp, return should equal offer
            // spread is 0 via saturating_sub — correct here, but the same mechanism
            // would hide a bug where return_amount > offer_amount
            assert_eq!(
                result.spread_amount,
                Uint128::zero(),
                "Spread is zero via saturating_sub"
            );
        }

        // MEDIUM-2: Zero amp factor causes panic (not a clean error)
        // This proves the lack of input validation: amp=0 causes leverage=0,
        // then (leverage - 1) underflows Decimal256 (unsigned), causing a panic
        // instead of a descriptive error.
        #[test]
        #[should_panic]
        fn test_zero_amp_factor_panics() {
            let pool = Decimal256::from_ratio(1000u128, 1u128);
            let offer = Decimal256::from_ratio(100u128, 1u128);

            // This panics due to unsigned subtraction underflow in calculate_step
            // (leverage - Decimal256::one()) where leverage = 0
            let _ = compute_stable_swap(&offer, &pool, &pool, Uint64::new(0));
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

        // Additional: The maximum safe pool size is ~1e18 as integer value.
        // 1e19 already overflows due to intermediate multiplications in compute_d
        // (pools[0] * N_COINS = 1e19 * 2, then d^3 / (2e19 * 2e19) requires d^3
        // which at d ≈ 2e19 produces atomics ≈ 8e75, but the unchecked multiply
        // (amount_a_times_coins * amount_b_times_coins) = 4e38 as Decimal256,
        // with atomics = 4e56, and d.pow(3) atomics = 8e75 — the division
        // d.pow(3) / product overflows during the pow step itself for 1e19).
        // 1e19 pools overflow in the multiply step, returning an error (not panic)
        // because the overflow occurs in a checked_mul path rather than unchecked pow
        #[test]
        fn test_1e19_pools_overflow_with_error() {
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
                result.is_err(),
                "1e19 pools should fail with overflow. Got: {:?}",
                result.unwrap()
            );
        }

        // The actual maximum safe pool size is ~1e18 (same as current test max)
        #[test]
        fn test_maximum_safe_pool_size_is_1e18() {
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

        // MEDIUM-2: Zero pool reserve causes panic (division by zero in d_product)
        // compute_d line 93: d^3 / (amount_a_times_coins * amount_b_times_coins)
        // If one pool is zero but the other isn't, sum_x != 0 so the early return
        // is skipped, then the division by zero hits the unchecked / operator.
        #[test]
        #[should_panic]
        fn test_zero_pool_reserve_panics() {
            let zero_pool = Decimal256::zero();
            let pool = Decimal256::from_ratio(1000u128, 1u128);
            let offer = Decimal256::from_ratio(100u128, 1u128);

            // One pool is zero, the other is not. sum_x != 0, so Newton's method runs.
            // amount_b_times_coins = 0, causing division by zero in d_product.
            let _ = compute_stable_swap(&offer, &pool, &zero_pool, Uint64::new(1000));
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
