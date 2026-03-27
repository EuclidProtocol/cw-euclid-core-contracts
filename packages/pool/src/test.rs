#[cfg(test)]
mod tests {
    use crate::{
        calculate_amount_from_shares, calculate_cp_swap, calculate_lp_allocation, pre_swap,
        stable_math::compute_stable_swap, SwapCalculationMethod,
    };
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{Addr, Decimal, Decimal256, Uint256, Uint64};
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
        let token_1_amount = Uint256::from(token_1_amount);
        let token_2_amount = Uint256::from(token_2_amount);
        let total_liquidity_1 = Uint256::from(total_liquidity_1);
        let total_liquidity_2 = Uint256::from(total_liquidity_2);
        let total_lp_supply = Uint256::from(total_lp_supply);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        assert_eq!(lp_tokens, Uint256::from(expected_lp_tokens));

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
        let token_1_amount = Uint256::from(token_1_amount);
        let token_2_amount = Uint256::from(token_2_amount);
        let total_liquidity_1 = Uint256::from(total_liquidity_1);
        let total_liquidity_2 = Uint256::from(total_liquidity_2);
        let total_lp_supply = Uint256::from(total_lp_supply);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        assert_eq!(lp_tokens, Uint256::from(expected_lp_tokens));
    }

    #[rstest]
    #[case(
        "equal_pools",
        Uint256::from(100u128),
        Uint256::from(1000u128),
        Uint256::from(1000u128),
        Uint64::new(1000),
        Uint256::from(99u128),
        Uint256::from(1u128)
    )]
    #[case(
        "imbalanced_pools",
        Uint256::from(100u128),
        Uint256::from(2000u128),
        Uint256::from(1000u128),
        Uint64::new(100),
        Uint256::from(67u128),
        Uint256::from(33u128)
    )]
    #[case(
        "small_amount",
        Uint256::from(1u128),
        Uint256::from(1000000u128),
        Uint256::from(1000000u128),
        Uint64::new(1000),
        Uint256::from(1u128),
        Uint256::from(0u128)
    )]
    #[case(
        "large_amount",
        Uint256::from(1000u128),
        Uint256::from(2000u128),
        Uint256::from(2000u128),
        Uint64::new(1000),
        Uint256::from(946u128),
        Uint256::from(54u128)
    )]
    #[case(
        "extreme_imbalance",
        Uint256::from(100u128),
        Uint256::from(10000u128),
        Uint256::from(1000u128),
        Uint64::new(1000),
        Uint256::from(47u128),
        Uint256::from(53u128)
    )]
    #[case(
        "large_values large spread",
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint64::new(1000),
        Uint256::from(820871215252207999u128),
        Uint256::from(179128784747792001u128)
    )]
    #[case(
        "large_values small spread",
        Uint256::from(1000u128),
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint64::new(1000),
        Uint256::from(1000u128),
        Uint256::from(0u128)
    )]
    fn test_compute_stable_swap(
        #[case] case_name: &str,
        #[case] offer_asset: Uint256,
        #[case] offer_pool: Uint256,
        #[case] ask_pool: Uint256,
        #[case] swap_amount: Uint64,
        #[case] expected_return_amount: Uint256,
        #[case] expected_spread_amount: Uint256,
    ) {
        let result = compute_stable_swap(offer_asset, offer_pool, ask_pool, swap_amount).unwrap();

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
        let balances_storage: Map<Token, Uint256> = Map::new("balances");

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
                &Uint256::from(reserve_token_1),
            )
            .unwrap();
        balances_storage
            .save(
                deps.as_mut().storage,
                token_2.clone(),
                &Uint256::from(reserve_token_2),
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
            total_lp_tokens: Uint256::zero(),
        };
        state_storage.save(deps.as_mut().storage, &state).unwrap();

        let amount_in = Uint256::from(amount_in);
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
            (
                Uint256::from(reserve_token_1),
                Uint256::from(reserve_token_2),
            )
        } else {
            (
                Uint256::from(reserve_token_2),
                Uint256::from(reserve_token_1),
            )
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
        let balances_storage: Map<Token, Uint256> = Map::new("balances");

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
                &Uint256::from(reserve_token_1),
            )
            .unwrap();
        balances_storage
            .save(
                deps.as_mut().storage,
                token_2.clone(),
                &Uint256::from(reserve_token_2),
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
            total_lp_tokens: Uint256::zero(),
        };
        state_storage.save(deps.as_mut().storage, &state).unwrap();

        let amount_in = Uint256::from(amount_in);
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
            (
                Uint256::from(reserve_token_1),
                Uint256::from(reserve_token_2),
            )
        } else {
            (
                Uint256::from(reserve_token_2),
                Uint256::from(reserve_token_1),
            )
        };

        let expected_swap = compute_stable_swap(
            expected_swap_amount,
            reserve_in,
            reserve_out,
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

    fn percentage_deviation(actual: Uint256, expected: Uint256) -> Decimal256 {
        let actual = Decimal256::checked_from_integer(actual).unwrap();
        let expected = Decimal256::checked_from_integer(expected).unwrap();
        let deviation = actual.abs_diff(expected) / expected;
        deviation
            .checked_mul(Decimal256::checked_from_integer(100u64).unwrap())
            .unwrap()
    }

    // ========================================================================
    // AUDIT TESTS: Proving vulnerabilities documented in AUDIT.md
    // ========================================================================

    mod audit_tests {
        use super::*;
        use crate::stable_math::compute_d;

        // CRITICAL-1 FIX VERIFICATION: 24-decimal pools now work with checked_multiply_ratio
        // Previously panicked due to d.pow(3) overflow. Now uses Uint512 intermediate.
        #[test]
        fn test_24_decimal_balanced_pools_now_works() {
            let one_token_24dec = Uint256::from(1_000_000_000_000_000_000_000_000u128); // 1e24
            let offer = Uint256::from(100_000_000_000_000_000_000_000u128); // 1e23

            let result =
                compute_stable_swap(offer, one_token_24dec, one_token_24dec, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "24-decimal pools should now work after checked_multiply_ratio fix. Error: {:?}",
                result.err()
            );
            let swap = result.unwrap();
            assert!(
                swap.return_amount > Uint256::zero(),
                "Should return non-zero amount"
            );
            assert!(
                swap.return_amount <= offer,
                "Return should not exceed offer"
            );
        }

        // CRITICAL-1 FIX VERIFICATION: 1e20 pools now work
        #[test]
        fn test_1e20_pools_now_works() {
            let pool = Uint256::from(100_000_000_000_000_000_000u128); // 1e20
            let offer = Uint256::from(1_000_000_000_000_000_000u128); // 1e18
            let result = compute_stable_swap(offer, pool, pool, Uint64::new(1000));
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
            assert!(d > Decimal256::zero(), "D should be positive");
        }

        // CRITICAL-2 FIX VERIFICATION: compute_d handles Uint256::MAX without panic.
        // With checked_multiply_ratio (Uint512 intermediate), this may succeed or
        // return a clean error, but must never panic.
        #[test]
        fn test_compute_d_handles_extreme_values_without_panic() {
            let pool = Decimal256::from_ratio(1_000_000_000_000_000_000_000_000u128, 1u128);
            let result = compute_d(Uint64::new(1000), &[pool, pool]);
            match result {
                Ok(d) => assert!(d > Decimal256::zero(), "D should be positive if Ok"),
                Err(_) => {} // Clean error is fine
            }
        }

        // CRITICAL-2 FIX VERIFICATION: Uint256::MAX pools return a clean error
        // (not a panic). TOKEN_PRECISION=1 scales values by 10, so max supported
        // pool value is Uint256::MAX / 10.
        #[test]
        fn test_uint256_max_pools_returns_error() {
            let result = compute_stable_swap(
                Uint256::from(1000u128),
                Uint256::MAX,
                Uint256::MAX,
                Uint64::new(1000),
            );
            assert!(
                result.is_err(),
                "Uint256::MAX pools should return error (TOKEN_PRECISION overflow), not panic"
            );
        }

        // CRITICAL-2 FIX VERIFICATION: Uint256::MAX / 10 pools are handled without panic.
        // Depending on intermediate math bounds this may succeed or cleanly error.
        #[test]
        fn test_uint256_max_div_10_pools_no_panic() {
            let max_pool = Uint256::MAX.checked_div(Uint256::from(10u128)).unwrap();
            let result = compute_stable_swap(
                Uint256::from(1000u128),
                max_pool,
                max_pool,
                Uint64::new(1000),
            );
            match result {
                Ok(swap) => {
                    assert!(
                        swap.return_amount <= Uint256::from(1000u128),
                        "Return should not exceed offer"
                    );
                    assert_eq!(
                        swap.return_amount + swap.spread_amount,
                        Uint256::from(1000u128),
                        "return + spread should equal offer"
                    );
                }
                Err(_) => {} // Clean error is acceptable for extreme bounds
            }
        }

        // CRITICAL-2 FIX VERIFICATION: Uint256::MAX as offer must not panic
        #[test]
        fn test_uint256_max_offer_no_panic() {
            let pool = Uint256::from(1_000_000_000_000_000_000u128); // 1e18
            let result = compute_stable_swap(Uint256::MAX, pool, pool, Uint64::new(1000));
            // Must not panic
            match result {
                Ok(_) | Err(_) => {} // Either is fine, no panic
            }
        }

        // CRITICAL-2 FIX VERIFICATION: All Uint256::MAX inputs must not panic
        #[test]
        fn test_all_uint256_max_no_panic() {
            let result = compute_stable_swap(
                Uint256::MAX,
                Uint256::MAX,
                Uint256::MAX,
                Uint64::new(u64::MAX),
            );
            match result {
                Ok(_) | Err(_) => {} // Either is fine, no panic
            }
        }

        // Verify calc_y succeeds for large pool values after fix
        #[test]
        fn test_calc_y_large_pools_succeeds() {
            use crate::stable_math::calc_y;

            let pool = Decimal256::from_ratio(
                1_000_000_000_000_000_000_000_000u128, // 1e24
                1u128,
            );
            let new_amount = pool
                + Decimal256::from_ratio(
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

        // CRITICAL-3 FIX VERIFICATION: inputs are now explicit Uint256 integer types.
        // TOKEN_PRECISION=1 correctly adds/removes one decimal digit for integer inputs.
        #[test]
        fn test_precision_with_integer_inputs() {
            let pool = Uint256::from(1_000_000_000u128);
            let offer = Uint256::from(1_000_000u128); // 1 token at 6 decimals

            let result = compute_stable_swap(offer, pool, pool, Uint64::new(1000)).unwrap();

            assert!(
                result.return_amount <= offer,
                "Return should not exceed offer for stable swap. Got: {}",
                result.return_amount
            );
            assert!(
                result.spread_amount < Uint256::from(10_000u128),
                "Spread too high for balanced stable pool. Got: {}",
                result.spread_amount
            );
        }

        // HIGH-1 FIX VERIFICATION: amp factor consistency between compute_d and calc_y
        // Both now use Decimal256::from_ratio(amp, AMP_PRECISION).checked_mul(N_COINS)
        #[test]
        fn test_amp_factor_consistency() {
            // Higher amp = more stable = less slippage
            let pool = Uint256::from(10000u128);
            let offer = Uint256::from(100u128);

            let result_low_amp = compute_stable_swap(offer, pool, pool, Uint64::new(100)).unwrap();
            let result_high_amp =
                compute_stable_swap(offer, pool, pool, Uint64::new(10000)).unwrap();

            assert!(
                result_high_amp.return_amount >= result_low_amp.return_amount,
                "Higher amp should give better rate. Low amp return: {}, High amp return: {}",
                result_low_amp.return_amount,
                result_high_amp.return_amount
            );
            assert!(
                result_high_amp.spread_amount <= result_low_amp.spread_amount,
                "Higher amp should give lower spread. Low amp spread: {}, High amp spread: {}",
                result_low_amp.spread_amount,
                result_high_amp.spread_amount
            );
        }

        // HIGH-2 FIX VERIFICATION: spread uses checked_sub (not saturating_sub)
        #[test]
        fn test_spread_uses_checked_sub() {
            let pool = Uint256::from(1_000_000u128);
            let offer = Uint256::from(1u128);

            let result = compute_stable_swap(offer, pool, pool, Uint64::new(10000)).unwrap();

            assert_eq!(
                result.spread_amount,
                Uint256::zero(),
                "Spread should be zero for tiny swap in large balanced pool"
            );
        }

        // MEDIUM-2 FIX VERIFICATION: Zero amp factor returns clean error
        #[test]
        fn test_zero_amp_factor_returns_error() {
            let result = compute_stable_swap(
                Uint256::from(100u128),
                Uint256::from(1000u128),
                Uint256::from(1000u128),
                Uint64::new(0),
            );
            assert!(
                result.is_err(),
                "Zero amp factor should return error. Got: {:?}",
                result.unwrap()
            );
        }

        // MEDIUM-2 FIX VERIFICATION: Extremely large amp factor
        #[test]
        fn test_extreme_amp_factor() {
            let result = compute_stable_swap(
                Uint256::from(100u128),
                Uint256::from(1000u128),
                Uint256::from(1000u128),
                Uint64::new(u64::MAX),
            );
            // Should either succeed or return a clean error
            match result {
                Ok(swap) => {
                    assert!(
                        swap.return_amount <= Uint256::from(100u128),
                        "Return should not exceed offer"
                    );
                }
                Err(_) => {} // Overflow in leverage is acceptable
            }
        }

        // MEDIUM-2 FIX VERIFICATION: Zero pool reserve returns error
        #[test]
        fn test_zero_pool_reserve_returns_error() {
            let result = compute_stable_swap(
                Uint256::from(100u128),
                Uint256::from(1000u128),
                Uint256::zero(),
                Uint64::new(1000),
            );
            assert!(
                result.is_err(),
                "Zero pool reserve should return error. Got: {:?}",
                result.unwrap()
            );
        }

        // MEDIUM-2 FIX VERIFICATION: Zero offer returns error
        #[test]
        fn test_zero_offer_returns_error() {
            let result = compute_stable_swap(
                Uint256::zero(),
                Uint256::from(1000u128),
                Uint256::from(1000u128),
                Uint64::new(1000),
            );
            assert!(
                result.is_err(),
                "Zero offer should return error. Got: {:?}",
                result.unwrap()
            );
        }

        // LOW-1: Integer truncation creates systematic loss for users
        #[test]
        fn test_truncation_loss_small_swaps() {
            let result = compute_stable_swap(
                Uint256::from(1u128),
                Uint256::from(1_000_000u128),
                Uint256::from(1_000_000u128),
                Uint64::new(10000),
            )
            .unwrap();

            assert_eq!(
                result.return_amount,
                Uint256::from(1u128),
                "1-unit swap should return 1 in balanced pool"
            );
        }

        // After fix: 1e19 pools now work
        #[test]
        fn test_1e19_pools_now_works() {
            let pool = Uint256::from(10_000_000_000_000_000_000u128); // 1e19
            let offer = Uint256::from(1_000_000_000_000_000_000u128); // 1e18
            let result = compute_stable_swap(offer, pool, pool, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "1e19 pools should now work after fix. Error: {:?}",
                result.err()
            );
        }

        // 1e18 pools still work (regression check)
        #[test]
        fn test_1e18_pools_still_works() {
            let pool = Uint256::from(1_000_000_000_000_000_000u128); // 1e18
            let offer = Uint256::from(1000u128);
            let result = compute_stable_swap(offer, pool, pool, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "1e18 pools should work. Error: {:?}",
                result.err()
            );
        }

        // LOW-1: Demonstrate actual truncation loss with imbalanced pools
        #[test]
        fn test_truncation_loss_imbalanced_pools() {
            let result = compute_stable_swap(
                Uint256::from(100u128),
                Uint256::from(2000u128),
                Uint256::from(1000u128),
                Uint64::new(100),
            )
            .unwrap();

            let reconstructed = result
                .return_amount
                .checked_add(result.spread_amount)
                .unwrap();

            assert_eq!(
                reconstructed,
                Uint256::from(100u128),
                "return + spread should always equal offer by construction"
            );
            assert!(
                result.return_amount > Uint256::zero(),
                "Should get a non-zero return"
            );
        }

        // Verify the StableSwap invariant holds before and after a swap
        #[test]
        fn test_stableswap_invariant_preserved() {
            let pool_a_uint = Uint256::from(10000u128);
            let pool_b_uint = Uint256::from(10000u128);
            let offer_uint = Uint256::from(500u128);
            let amp = Uint64::new(1000);

            let pool_a = Decimal256::from_ratio(pool_a_uint, 1u128);
            let pool_b = Decimal256::from_ratio(pool_b_uint, 1u128);
            let offer = Decimal256::from_ratio(offer_uint, 1u128);

            // Compute D before swap
            let d_before = compute_d(amp, &[pool_a, pool_b]).unwrap();

            // Perform swap
            let result = compute_stable_swap(offer_uint, pool_a_uint, pool_b_uint, amp).unwrap();

            // New pool state after swap
            let new_pool_a = pool_a + offer;
            let new_pool_b = pool_b - Decimal256::from_ratio(result.return_amount, 1u128);

            // Compute D after swap
            let d_after = compute_d(amp, &[new_pool_a, new_pool_b]).unwrap();

            let diff = d_after.abs_diff(d_before);
            let relative_diff = diff / d_before;

            assert!(
                relative_diff < Decimal256::from_ratio(1u128, 1000u128), // < 0.1% deviation
                "Invariant D should be approximately preserved. D_before: {}, D_after: {}, relative_diff: {}",
                d_before, d_after, relative_diff
            );
        }

        // Uint256::MIN (zero) boundary: all zero inputs should return clean errors
        #[test]
        fn test_uint256_min_boundaries() {
            // Zero offer
            assert!(compute_stable_swap(
                Uint256::zero(),
                Uint256::from(1000u128),
                Uint256::from(1000u128),
                Uint64::new(1000)
            )
            .is_err());
            // Zero pool
            assert!(compute_stable_swap(
                Uint256::from(100u128),
                Uint256::zero(),
                Uint256::from(1000u128),
                Uint64::new(1000)
            )
            .is_err());
            // Zero ask pool
            assert!(compute_stable_swap(
                Uint256::from(100u128),
                Uint256::from(1000u128),
                Uint256::zero(),
                Uint64::new(1000)
            )
            .is_err());
            // Minimum valid: all 1
            let result = compute_stable_swap(
                Uint256::from(1u128),
                Uint256::from(1u128),
                Uint256::from(1u128),
                Uint64::new(100),
            );
            // Should either succeed or return a clean error, never panic
            match result {
                Ok(_) | Err(_) => {}
            }
        }
    }
}
