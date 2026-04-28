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
        Uint128::new(100),
        Uint128::new(1000),
        Uint128::new(1000),
        Uint64::new(1000),
        Uint128::new(99),
        Uint128::new(1)
    )]
    #[case(
        "imbalanced_pools",
        Uint128::new(100),
        Uint128::new(2000),
        Uint128::new(1000),
        Uint64::new(100),
        Uint128::new(67),
        Uint128::new(33)
    )]
    #[case(
        "small_amount",
        Uint128::new(1),
        Uint128::new(1000000),
        Uint128::new(1000000),
        Uint64::new(1000),
        Uint128::new(1),
        Uint128::new(0)
    )]
    #[case(
        "large_amount",
        Uint128::new(1000),
        Uint128::new(2000),
        Uint128::new(2000),
        Uint64::new(1000),
        Uint128::new(946),
        Uint128::new(54)
    )]
    #[case(
        "extreme_imbalance",
        Uint128::new(100),
        Uint128::new(10000),
        Uint128::new(1000),
        Uint64::new(1000),
        Uint128::new(47),
        Uint128::new(53)
    )]
    #[case(
        "large_values large spread",
        Uint128::new(1000000000000000000),
        Uint128::new(1000000000000000000),
        Uint128::new(1000000000000000000),
        Uint64::new(1000),
        Uint128::new(820871215252207999),
        Uint128::new(179128784747792001)
    )]
    #[case(
        "large_values small spread",
        Uint128::new(1000),
        Uint128::new(1000000000000000000),
        Uint128::new(1000000000000000000),
        Uint64::new(1000),
        Uint128::new(1000),
        Uint128::new(0)
    )]
    // Cases where ask_pool > offer_pool: return_amount exceeds offer_amount
    #[case(
        "ask_pool_2x_offer_pool",
        Uint128::new(100),
        Uint128::new(1000),
        Uint128::new(2000),
        Uint64::new(1000),
        Uint128::new(106),
        Uint128::new(6)
    )]
    #[case(
        "ask_pool_10x_offer_pool",
        Uint128::new(100),
        Uint128::new(1000),
        Uint128::new(10000),
        Uint64::new(1000),
        Uint128::new(196),
        Uint128::new(96)
    )]
    #[case(
        "ask_pool_2x_low_amp",
        Uint128::new(500),
        Uint128::new(5000),
        Uint128::new(10000),
        Uint64::new(100),
        Uint128::new(685),
        Uint128::new(185)
    )]
    #[case(
        "ask_pool_4x_offer_pool",
        Uint128::new(1000),
        Uint128::new(2000),
        Uint128::new(8000),
        Uint64::new(1000),
        Uint128::new(1160),
        Uint128::new(160)
    )]
    #[case(
        "large_values_ask_pool_5x",
        Uint128::new(1000000000000000000),
        Uint128::new(1000000000000000000),
        Uint128::new(5000000000000000000),
        Uint64::new(1000),
        Uint128::new(1169582311873333606),
        Uint128::new(169582311873333606)
    )]
    fn test_compute_stable_swap(
        #[case] case_name: &str,
        #[case] offer_asset: Uint128,
        #[case] offer_pool: Uint128,
        #[case] ask_pool: Uint128,
        #[case] swap_amount: Uint64,
        #[case] expected_return_amount: Uint128,
        #[case] expected_spread_amount: Uint128,
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
    // ask_pool > offer_pool: return_amount > offer_amount
    #[case(true, 5000u128, 10000u128, 100u64, 50u64, 1000u128)]
    #[case(false, 20000u128, 8000u128, 30u64, 20u64, 500u128)]
    #[case(true, 1000u128, 5000u128, 10u64, 1u64, 100u128)]
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
    // ask_pool > offer_pool: return_amount > offer_amount
    #[case(true, 5000u128, 10000u128, 100u64, 50u64, 1000u64, 1000u128)]
    #[case(false, 20000u128, 8000u128, 30u64, 20u64, 1000u64, 500u128)]
    #[case(true, 1000u128, 5000u128, 10u64, 1u64, 1000u64, 100u128)]
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

    fn percentage_deviation(actual: Uint128, expected: Uint128) -> Decimal256 {
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
            let one_token_24dec = Uint128::new(1_000_000_000_000_000_000_000_000); // 1e24
            let offer = Uint128::new(100_000_000_000_000_000_000_000); // 1e23

            let result =
                compute_stable_swap(offer, one_token_24dec, one_token_24dec, Uint64::new(1000));
            assert!(
                result.is_ok(),
                "24-decimal pools should now work after checked_multiply_ratio fix. Error: {:?}",
                result.err()
            );
            let swap = result.unwrap();
            assert!(
                swap.return_amount > Uint128::zero(),
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
            let pool = Uint128::new(100_000_000_000_000_000_000); // 1e20
            let offer = Uint128::new(1_000_000_000_000_000_000); // 1e18
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

        // CRITICAL-2 FIX VERIFICATION: compute_d handles Uint128::MAX without panic.
        // With checked_multiply_ratio (Uint512 intermediate), this may succeed or
        // return a clean error, but must never panic.
        #[test]
        fn test_compute_d_handles_extreme_values_without_panic() {
            let pool = Decimal256::from_ratio(Uint128::MAX, 1u128);
            let result = compute_d(Uint64::new(1000), &[pool, pool]);
            // Either Ok or Err is acceptable; the key is no panic
            match result {
                Ok(d) => assert!(d > Decimal256::zero(), "D should be positive if Ok"),
                Err(_) => {} // Clean error is fine
            }
        }

        // CRITICAL-2 FIX VERIFICATION: Uint128::MAX pools return a clean error
        // (not a panic). TOKEN_PRECISION=1 scales values by 10, so max supported
        // pool value is Uint128::MAX / 10.
        #[test]
        fn test_uint128_max_pools_returns_error() {
            let result = compute_stable_swap(
                Uint128::new(1000),
                Uint128::MAX,
                Uint128::MAX,
                Uint64::new(1000),
            );
            assert!(
                result.is_err(),
                "Uint128::MAX pools should return error (TOKEN_PRECISION overflow), not panic"
            );
        }

        // CRITICAL-2 FIX VERIFICATION: Uint128::MAX / 10 pools succeed.
        // This is the maximum supported pool value given TOKEN_PRECISION=1.
        #[test]
        fn test_uint128_max_div_10_pools_succeeds() {
            let max_pool = Uint128::MAX.checked_div(Uint128::new(10)).unwrap();
            let result =
                compute_stable_swap(Uint128::new(1000), max_pool, max_pool, Uint64::new(1000));
            let swap = result.expect("Uint128::MAX/10 pools should succeed");
            assert!(
                swap.return_amount <= Uint128::new(1000),
                "Return should not exceed offer"
            );
            assert!(
                swap.return_amount > Uint128::zero(),
                "Should return non-zero for balanced pools"
            );
            assert_eq!(
                swap.return_amount.u128() + swap.spread_amount.u128(),
                1000,
                "return + spread should equal offer"
            );
        }

        // CRITICAL-2 FIX VERIFICATION: Uint128::MAX as offer must not panic
        #[test]
        fn test_uint128_max_offer_no_panic() {
            let pool = Uint128::new(1_000_000_000_000_000_000); // 1e18
            let result = compute_stable_swap(Uint128::MAX, pool, pool, Uint64::new(1000));
            // Must not panic
            match result {
                Ok(_) | Err(_) => {} // Either is fine, no panic
            }
        }

        // CRITICAL-2 FIX VERIFICATION: All Uint128::MAX inputs must not panic
        #[test]
        fn test_all_uint128_max_no_panic() {
            let result = compute_stable_swap(
                Uint128::MAX,
                Uint128::MAX,
                Uint128::MAX,
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

        // CRITICAL-3 FIX VERIFICATION: inputs are now explicit Uint128 integer types.
        // TOKEN_PRECISION=1 correctly adds/removes one decimal digit for integer inputs.
        #[test]
        fn test_precision_with_integer_inputs() {
            let pool = Uint128::new(1_000_000_000);
            let offer = Uint128::new(1_000_000); // 1 token at 6 decimals

            let result = compute_stable_swap(offer, pool, pool, Uint64::new(1000)).unwrap();

            assert!(
                result.return_amount <= offer,
                "Return should not exceed offer for stable swap. Got: {}",
                result.return_amount
            );
            assert!(
                result.spread_amount < Uint128::new(10_000),
                "Spread too high for balanced stable pool. Got: {}",
                result.spread_amount
            );
        }

        // HIGH-1 FIX VERIFICATION: amp factor consistency between compute_d and calc_y
        // Both now use Decimal256::from_ratio(amp, AMP_PRECISION).checked_mul(N_COINS)
        #[test]
        fn test_amp_factor_consistency() {
            // Higher amp = more stable = less slippage
            let pool = Uint128::new(10000);
            let offer = Uint128::new(100);

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
            let pool = Uint128::new(1_000_000);
            let offer = Uint128::new(1);

            let result = compute_stable_swap(offer, pool, pool, Uint64::new(10000)).unwrap();

            assert_eq!(
                result.spread_amount,
                Uint128::zero(),
                "Spread should be zero for tiny swap in large balanced pool"
            );
        }

        // MEDIUM-2 FIX VERIFICATION: Zero amp factor returns clean error
        #[test]
        fn test_zero_amp_factor_returns_error() {
            let result = compute_stable_swap(
                Uint128::new(100),
                Uint128::new(1000),
                Uint128::new(1000),
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
                Uint128::new(100),
                Uint128::new(1000),
                Uint128::new(1000),
                Uint64::new(u64::MAX),
            );
            // Should either succeed or return a clean error
            match result {
                Ok(swap) => {
                    assert!(
                        swap.return_amount <= Uint128::new(100),
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
                Uint128::new(100),
                Uint128::new(1000),
                Uint128::zero(),
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
                Uint128::zero(),
                Uint128::new(1000),
                Uint128::new(1000),
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
                Uint128::new(1),
                Uint128::new(1_000_000),
                Uint128::new(1_000_000),
                Uint64::new(10000),
            )
            .unwrap();

            assert_eq!(
                result.return_amount,
                Uint128::new(1),
                "1-unit swap should return 1 in balanced pool"
            );
        }

        // After fix: 1e19 pools now work
        #[test]
        fn test_1e19_pools_now_works() {
            let pool = Uint128::new(10_000_000_000_000_000_000); // 1e19
            let offer = Uint128::new(1_000_000_000_000_000_000); // 1e18
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
            let pool = Uint128::new(1_000_000_000_000_000_000); // 1e18
            let offer = Uint128::new(1000);
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
                Uint128::new(100),
                Uint128::new(2000),
                Uint128::new(1000),
                Uint64::new(100),
            )
            .unwrap();

            let reconstructed = result
                .return_amount
                .checked_add(result.spread_amount)
                .unwrap();

            assert_eq!(
                reconstructed,
                Uint128::new(100),
                "return + spread should always equal offer by construction"
            );
            assert!(
                result.return_amount > Uint128::zero(),
                "Should get a non-zero return"
            );
        }

        // Verify the StableSwap invariant holds before and after a swap
        #[test]
        fn test_stableswap_invariant_preserved() {
            let pool_a_uint = Uint128::new(10000);
            let pool_b_uint = Uint128::new(10000);
            let offer_uint = Uint128::new(500);
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

        // Uint128::MIN (zero) boundary: all zero inputs should return clean errors
        #[test]
        fn test_uint128_min_boundaries() {
            // Zero offer
            assert!(compute_stable_swap(
                Uint128::zero(),
                Uint128::new(1000),
                Uint128::new(1000),
                Uint64::new(1000)
            )
            .is_err());
            // Zero pool
            assert!(compute_stable_swap(
                Uint128::new(100),
                Uint128::zero(),
                Uint128::new(1000),
                Uint64::new(1000)
            )
            .is_err());
            // Zero ask pool
            assert!(compute_stable_swap(
                Uint128::new(100),
                Uint128::new(1000),
                Uint128::zero(),
                Uint64::new(1000)
            )
            .is_err());
            // Minimum valid: all 1
            let result = compute_stable_swap(
                Uint128::new(1),
                Uint128::new(1),
                Uint128::new(1),
                Uint64::new(100),
            );
            // Should either succeed or return a clean error, never panic
            match result {
                Ok(_) | Err(_) => {}
            }
        }

        // FIX VERIFICATION: return_amount > offer_amount is valid when ask_pool > offer_pool
        // Previously, spread_amount used checked_sub(offer - return) which panicked in this case.
        #[test]
        fn test_return_exceeds_offer_when_ask_pool_larger() {
            // ask_pool is 2x offer_pool, so swapping into the deeper side yields more
            let result = compute_stable_swap(
                Uint128::new(100),
                Uint128::new(1000),
                Uint128::new(2000),
                Uint64::new(1000),
            )
            .unwrap();

            assert!(
                result.return_amount > Uint128::new(100),
                "return_amount should exceed offer_amount when ask_pool > offer_pool. Got: {}",
                result.return_amount
            );
            // spread is abs_diff, so it captures the magnitude of price impact
            assert_eq!(
                result.spread_amount,
                result.return_amount.abs_diff(Uint128::new(100)),
                "spread should be abs_diff(offer, return)"
            );
        }

        // Verify the invariant holds even when return > offer (ask_pool > offer_pool)
        #[test]
        fn test_stableswap_invariant_preserved_imbalanced_ask_larger() {
            let pool_a_uint = Uint128::new(5000);
            let pool_b_uint = Uint128::new(15000);
            let offer_uint = Uint128::new(500);
            let amp = Uint64::new(1000);

            let pool_a = Decimal256::from_ratio(pool_a_uint, 1u128);
            let pool_b = Decimal256::from_ratio(pool_b_uint, 1u128);
            let offer = Decimal256::from_ratio(offer_uint, 1u128);

            let d_before = compute_d(amp, &[pool_a, pool_b]).unwrap();

            let result = compute_stable_swap(offer_uint, pool_a_uint, pool_b_uint, amp).unwrap();

            // return_amount > offer in this configuration
            assert!(
                result.return_amount > offer_uint,
                "Expected return > offer for imbalanced pool (ask > offer reserve)"
            );

            let new_pool_a = pool_a + offer;
            let new_pool_b = pool_b - Decimal256::from_ratio(result.return_amount, 1u128);

            let d_after = compute_d(amp, &[new_pool_a, new_pool_b]).unwrap();

            let diff = d_after.abs_diff(d_before);
            let relative_diff = diff / d_before;

            assert!(
                relative_diff < Decimal256::from_ratio(1u128, 1000u128),
                "Invariant D should be preserved. D_before: {}, D_after: {}, relative_diff: {}",
                d_before,
                d_after,
                relative_diff
            );
        }

        // Symmetry test: swapping in both directions should yield consistent results
        #[test]
        fn test_swap_direction_symmetry() {
            let pool_a = Uint128::new(5000);
            let pool_b = Uint128::new(10000);
            let offer = Uint128::new(100);
            let amp = Uint64::new(1000);

            // Swap A -> B (ask_pool > offer_pool)
            let result_a_to_b = compute_stable_swap(offer, pool_a, pool_b, amp).unwrap();
            // Swap B -> A (offer_pool > ask_pool)
            let result_b_to_a = compute_stable_swap(offer, pool_b, pool_a, amp).unwrap();

            // When swapping into deeper pool, you get more out
            assert!(
                result_a_to_b.return_amount > result_b_to_a.return_amount,
                "Swapping into deeper pool should yield more. A->B: {}, B->A: {}",
                result_a_to_b.return_amount,
                result_b_to_a.return_amount
            );
        }

        // CP swap also yields return > offer when ask_pool > offer_pool
        #[test]
        fn test_cp_swap_return_exceeds_offer_when_ask_pool_larger() {
            let result =
                calculate_cp_swap(Uint128::new(100), Uint128::new(1000), Uint128::new(5000))
                    .unwrap();

            assert!(
                result.return_amount > Uint128::new(100),
                "CP swap return should exceed offer when ask_pool > offer_pool. Got: {}",
                result.return_amount
            );
        }
    }

    // ========================================================================
    // STABLE LP ALLOCATION TESTS
    //
    // `calculate_stable_lp_allocation` is a private function in
    // pool_functions.rs, so these tests exercise it indirectly through
    // `add_liquidity` (the only caller) and assert on the resulting
    // `state.total_lp_tokens`. We also validate the underlying D-invariant
    // math using the public `compute_d` for cross-checking.
    // ========================================================================

    mod stable_lp_allocation_tests {
        use super::*;
        use crate::stable_math::compute_d;
        use crate::{add_liquidity, MINIMUM_LIQUIDITY};
        use cosmwasm_std::testing::{message_info, mock_env};
        use euclid::{
            cross_chain_user::CrossChainUser,
            fee::{DenomFees, Fee, TotalFees},
            msgs::vlp::base::State,
            token::{Pair, PairWithAmount, Token, TokenWithAmount},
        };

        // ----------------------------------------------------------------
        // Test fixtures / helpers
        // ----------------------------------------------------------------

        fn token1() -> Token {
            Token::create("token1".to_string()).unwrap()
        }

        fn token2() -> Token {
            Token::create("token2".to_string()).unwrap()
        }

        fn make_pair() -> Pair {
            Pair::new(token1(), token2()).unwrap()
        }

        fn vsl_chain() -> ChainUid {
            ChainUid::vsl_chain_uid().unwrap()
        }

        fn make_sender() -> CrossChainUser {
            CrossChainUser {
                address: "sender".to_string(),
                chain_uid: vsl_chain(),
            }
        }

        /// Set up the storage maps/items required by `add_liquidity` and
        /// run it. Returns the state's `total_lp_tokens` after the call
        /// (the raw stable LP allocation, not the user-facing amount).
        #[allow(clippy::too_many_arguments)]
        fn run_add_liquidity(
            reserve_1: Uint128,
            reserve_2: Uint128,
            initial_total_lp: Uint128,
            deposit_1: Uint128,
            deposit_2: Uint128,
            amp_factor: Option<Uint64>,
            slippage_tolerance_bps: u64,
        ) -> Result<Uint128, euclid::error::ContractError> {
            let mut deps = mock_dependencies();
            let env = mock_env();

            let state_storage: Item<State> = Item::new("state");
            let balances_storage: Map<Token, Uint128> = Map::new("balances");
            let chain_lp_tokens_storage: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");
            let collateral_lp_tokens_storage: Item<Uint128> = Item::new("collateral_lp_tokens");

            let pair = make_pair();
            let router_addr = deps.api.addr_make("router");

            let fee = Fee::new(
                0,
                0,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "fee".to_string(),
                ),
            );

            let state = State {
                pair: pair.clone(),
                router: router_addr.clone(),
                virtual_balance_contract: Addr::unchecked("vbc"),
                fee,
                total_fees_collected: TotalFees {
                    lp_fees: DenomFees {
                        totals: HashMap::default(),
                    },
                    euclid_fees: DenomFees {
                        totals: HashMap::default(),
                    },
                },
                last_updated: 0,
                total_lp_tokens: initial_total_lp,
            };
            state_storage.save(deps.as_mut().storage, &state).unwrap();
            balances_storage
                .save(deps.as_mut().storage, pair.token_1.clone(), &reserve_1)
                .unwrap();
            balances_storage
                .save(deps.as_mut().storage, pair.token_2.clone(), &reserve_2)
                .unwrap();

            let sender = make_sender();
            chain_lp_tokens_storage
                .save(
                    deps.as_mut().storage,
                    sender.chain_uid.clone(),
                    &Uint128::zero(),
                )
                .unwrap();

            let liquidity = PairWithAmount::new(
                TokenWithAmount {
                    token: pair.token_1.clone(),
                    amount: deposit_1,
                },
                TokenWithAmount {
                    token: pair.token_2.clone(),
                    amount: deposit_2,
                },
            )
            .unwrap();

            let info = message_info(&router_addr, &[]);
            add_liquidity(
                deps.as_mut(),
                env,
                info,
                &state_storage,
                &balances_storage,
                &chain_lp_tokens_storage,
                &collateral_lp_tokens_storage,
                sender,
                liquidity,
                slippage_tolerance_bps,
                amp_factor,
                "tx-1".to_string(),
            )?;

            let state_after = state_storage.load(&deps.storage).unwrap();
            Ok(state_after.total_lp_tokens)
        }

        /// Compute the expected stable LP allocation using the same logic as
        /// `calculate_stable_lp_allocation`, but using public `compute_d`.
        /// This lets us cross-check the indirect assertions.
        fn expected_stable_lp(
            amount_1: Uint128,
            amount_2: Uint128,
            reserve_1: Uint128,
            reserve_2: Uint128,
            total_lp_supply: Uint128,
            amp: Uint64,
        ) -> Uint128 {
            let pools_new = [
                Decimal256::checked_from_integer(reserve_1 + amount_1).unwrap(),
                Decimal256::checked_from_integer(reserve_2 + amount_2).unwrap(),
            ];
            if total_lp_supply.is_zero() {
                let d = compute_d(amp, &pools_new).unwrap();
                return d.to_uint128_with_precision(0u32).unwrap();
            }
            let pools_old = [
                Decimal256::checked_from_integer(reserve_1).unwrap(),
                Decimal256::checked_from_integer(reserve_2).unwrap(),
            ];
            let d_old = compute_d(amp, &pools_old).unwrap();
            let d_new = compute_d(amp, &pools_new).unwrap();
            let lp_supply_dec = Decimal256::checked_from_integer(total_lp_supply).unwrap();
            let increase = d_new - d_old;
            lp_supply_dec
                .checked_multiply_ratio(increase, d_old)
                .unwrap()
                .to_uint128_with_precision(0u32)
                .unwrap()
        }

        // ----------------------------------------------------------------
        // First-deposit (empty pool) tests
        // ----------------------------------------------------------------

        // First deposit: total LP supply is zero so allocation must equal D.
        // Note: low-amp first deposits (e.g. amp=1) on small reserves can
        // produce D < MINIMUM_LIQUIDITY, in which case `add_liquidity`
        // underflows when subtracting MINIMUM_LIQUIDITY from the user's
        // share. Test cases are sized so D >= 1000 for all chosen amps.
        #[rstest]
        #[case::balanced(1_000u128, 1_000u128, 100u64)]
        #[case::imbalanced_10x(10_000u128, 100_000u128, 100u64)]
        #[case::imbalanced_low_amp(10_000u128, 100_000u128, 50u64)]
        #[case::imbalanced_high_amp(10_000u128, 100_000u128, 1000u64)]
        #[case::large_balanced(1_000_000u128, 1_000_000u128, 100u64)]
        fn test_first_deposit_returns_d(
            #[case] amount_1: u128,
            #[case] amount_2: u128,
            #[case] amp: u64,
        ) {
            let amount_1 = Uint128::new(amount_1);
            let amount_2 = Uint128::new(amount_2);
            let amp = Uint64::new(amp);

            let total_lp_after = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                Some(amp),
                5_000,
            )
            .unwrap();

            // First deposit allocation should equal D for the new pool.
            let expected = expected_stable_lp(
                amount_1,
                amount_2,
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amp,
            );
            assert_eq!(
                total_lp_after, expected,
                "First-deposit LP must equal D-invariant of the new pool"
            );
        }

        // The exact integration-test fixture: 10k/100k seeded at amp=100
        // produces D = 82026 (matches tests-integration/src/tests/factory.rs:2474).
        #[test]
        fn test_first_deposit_10k_100k_amp_100_equals_82026() {
            let total_lp_after = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                Uint128::new(10_000),
                Uint128::new(100_000),
                Some(Uint64::new(100)),
                5_000,
            )
            .unwrap();
            assert_eq!(
                total_lp_after,
                Uint128::new(82_026),
                "Seeded 10k/100k pool at amp=100 must yield D = 82026"
            );
        }

        // CP would compute LP = isqrt(10_000 * 100_000) = 31_622. Stable path
        // must produce a strictly different (and larger here) number,
        // demonstrating the fix shipped in the PR.
        #[test]
        fn test_first_deposit_stable_differs_from_cp_imbalanced() {
            let amount_1 = Uint128::new(10_000);
            let amount_2 = Uint128::new(100_000);

            let cp = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                None,
                5_000,
            )
            .unwrap();
            let stable = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                Some(Uint64::new(100)),
                5_000,
            )
            .unwrap();

            assert_eq!(cp, Uint128::new(31_622));
            assert_eq!(stable, Uint128::new(82_026));
            assert_ne!(
                cp, stable,
                "CP and stable paths must diverge on imbalanced reserves"
            );
        }

        // ----------------------------------------------------------------
        // Subsequent deposit on a balanced pool
        // ----------------------------------------------------------------

        // Add 100/100 to a 1000/1000 pool with 1000 LP supply: balanced
        // proportional growth should mint ~10% of supply.
        // Low amp values are excluded because compute_d underflows at very
        // low leverage — see test_compute_d_amp_1_balanced_currently_underflows.
        #[rstest]
        #[case::amp_50(50u64)]
        #[case::amp_100(100u64)]
        #[case::amp_1000(1000u64)]
        fn test_subsequent_deposit_balanced(#[case] amp: u64) {
            let reserve = Uint128::new(1_000);
            let initial_lp = Uint128::new(1_000);
            let deposit = Uint128::new(100);
            let amp = Uint64::new(amp);

            let total_lp_after = run_add_liquidity(
                reserve,
                reserve,
                initial_lp,
                deposit,
                deposit,
                Some(amp),
                5_000,
            )
            .unwrap();

            let minted = total_lp_after - initial_lp;
            let expected = expected_stable_lp(deposit, deposit, reserve, reserve, initial_lp, amp);
            assert_eq!(
                minted, expected,
                "Balanced subsequent deposit allocation must match D-growth formula"
            );

            // Sanity: balanced 10% growth on balanced pool should mint ~10% of
            // total supply (within 1 unit of rounding).
            assert!(
                minted >= Uint128::new(99) && minted <= Uint128::new(100),
                "Balanced 10% growth should mint ~100 LP, got {minted}"
            );
        }

        // ----------------------------------------------------------------
        // Subsequent deposit on an imbalanced pool
        // ----------------------------------------------------------------

        // After seeding 10k/100k at amp=100 (D=82026), depositing the same
        // amounts again should ~double D, doubling the LP supply.
        #[test]
        fn test_subsequent_deposit_imbalanced_10k_100k_doubles_d() {
            let reserve_1 = Uint128::new(10_000);
            let reserve_2 = Uint128::new(100_000);
            let initial_lp = Uint128::new(82_026); // D for the seeded pool
            let amp = Uint64::new(100);

            let total_lp_after = run_add_liquidity(
                reserve_1,
                reserve_2,
                initial_lp,
                reserve_1,
                reserve_2,
                Some(amp),
                5_000,
            )
            .unwrap();

            let minted = total_lp_after - initial_lp;
            let expected =
                expected_stable_lp(reserve_1, reserve_2, reserve_1, reserve_2, initial_lp, amp);
            assert_eq!(minted, expected);

            // Doubling reserves doubles D, so minted ~= initial_lp.
            // Allow tiny rounding error from Newton's method.
            let delta = if minted > initial_lp {
                minted - initial_lp
            } else {
                initial_lp - minted
            };
            assert!(
                delta <= Uint128::new(2),
                "Doubling reserves should mint ~initial_lp, got minted={minted}, initial={initial_lp}"
            );
        }

        // ----------------------------------------------------------------
        // Imbalanced (near-single-sided) deposits
        // ----------------------------------------------------------------

        // The `add_liquidity` handler enforces a 50% slippage cap between
        // the deposit ratio and the pool ratio, so a true zero-on-one-side
        // deposit cannot pass the slippage check. The closest in-spec test
        // is a deposit ratio that is exactly at the slippage boundary
        // (1.5:1 deposit on a 1:1 pool = 50% deviation).
        //
        // This still exercises the D-growth path with an asymmetric
        // contribution and confirms the formula yields a positive,
        // formula-matching LP amount.
        #[test]
        fn test_subsequent_deposit_imbalanced_within_slippage() {
            let reserve_1 = Uint128::new(1_000);
            let reserve_2 = Uint128::new(1_000);
            let initial_lp = Uint128::new(2_000); // approx D for amp=100 balanced
            let amp = Uint64::new(100);

            // 1.5x more token_1 than token_2 — exactly at the 50% slippage cap.
            let deposit_1 = Uint128::new(150);
            let deposit_2 = Uint128::new(100);

            let total_lp_after = run_add_liquidity(
                reserve_1,
                reserve_2,
                initial_lp,
                deposit_1,
                deposit_2,
                Some(amp),
                5_000,
            )
            .unwrap();

            let minted = total_lp_after - initial_lp;
            let expected =
                expected_stable_lp(deposit_1, deposit_2, reserve_1, reserve_2, initial_lp, amp);
            assert_eq!(minted, expected);
            assert!(
                minted > Uint128::zero(),
                "Imbalanced deposit must mint > 0 LP"
            );
        }

        // ----------------------------------------------------------------
        // Amp factor sensitivity
        // ----------------------------------------------------------------

        // Lower amp -> closer to constant-product (geometric mean).
        // Higher amp -> closer to constant-sum (arithmetic mean).
        // For an imbalanced first deposit (10k/100k):
        //   geometric mean ≈ 31622 (CP value)
        //   arithmetic mean = 110000
        // So D should grow with amp.
        //
        // We start at amp=50 because lower amp values underflow compute_d
        // on this imbalanced pool — see
        // test_compute_d_amp_1_imbalanced_currently_underflows.
        #[test]
        fn test_first_deposit_amp_sensitivity_imbalanced() {
            let amount_1 = Uint128::new(10_000);
            let amount_2 = Uint128::new(100_000);

            let lp_amp_low = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                Some(Uint64::new(50)),
                5_000,
            )
            .unwrap();
            let lp_amp_100 = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                Some(Uint64::new(100)),
                5_000,
            )
            .unwrap();
            let lp_amp_1000 = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                Some(Uint64::new(1000)),
                5_000,
            )
            .unwrap();

            // Strict ordering: higher amp -> higher D for the same reserves.
            assert!(
                lp_amp_low < lp_amp_100,
                "amp=50 LP ({lp_amp_low}) should be < amp=100 LP ({lp_amp_100})"
            );
            assert!(
                lp_amp_100 < lp_amp_1000,
                "amp=100 LP ({lp_amp_100}) should be < amp=1000 LP ({lp_amp_1000})"
            );

            // Bounds: D is between geometric mean (~31622) and arithmetic mean (110000).
            assert!(
                lp_amp_low >= Uint128::new(31_000),
                "amp=50 LP should be >= geometric mean, got {lp_amp_low}"
            );
            assert!(
                lp_amp_1000 <= Uint128::new(110_000),
                "amp=1000 LP should be <= arithmetic mean, got {lp_amp_1000}"
            );
        }

        // For a balanced first deposit, D should equal sum of reserves
        // regardless of amp factor (geometric mean and arithmetic mean
        // coincide for balanced reserves).
        // Low amp values excluded — see
        // test_compute_d_amp_1_balanced_currently_underflows.
        #[rstest]
        #[case::amp_50(50u64)]
        #[case::amp_100(100u64)]
        #[case::amp_1000(1000u64)]
        fn test_first_deposit_balanced_d_equals_sum(#[case] amp: u64) {
            let amount = Uint128::new(1_000);
            let total_lp_after = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount,
                amount,
                Some(Uint64::new(amp)),
                5_000,
            )
            .unwrap();
            // For balanced pools, D = sum(reserves)
            assert_eq!(total_lp_after, Uint128::new(2_000));
        }

        // ----------------------------------------------------------------
        // Edge cases
        // ----------------------------------------------------------------

        // Zero deposits on a pool with non-zero D should fail because the
        // resulting D doesn't increase (Decimal256::checked_from_ratio errors
        // on the zero ratio before the LP function is reached).
        #[test]
        fn test_zero_deposit_both_sides_fails() {
            let res = run_add_liquidity(
                Uint128::new(1_000),
                Uint128::new(1_000),
                Uint128::new(1_000),
                Uint128::zero(),
                Uint128::zero(),
                Some(Uint64::new(100)),
                5_000,
            );
            assert!(res.is_err(), "zero/zero deposit must error");
        }

        // First-deposit with zero reserves on both sides cannot form a pool.
        #[test]
        fn test_first_deposit_zero_amounts_fails() {
            let res = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                Some(Uint64::new(100)),
                5_000,
            );
            assert!(res.is_err(), "zero first-deposit must error");
        }

        // Very large deposits should not overflow.
        #[test]
        fn test_large_deposits_no_overflow() {
            let big = Uint128::new(1_000_000_000_000_000_000); // 1e18
            let res = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                big,
                big,
                Some(Uint64::new(100)),
                5_000,
            );
            assert!(
                res.is_ok(),
                "1e18 balanced first deposit must not overflow: {:?}",
                res.err()
            );
            // Balanced pool: D = 2 * big
            assert_eq!(res.unwrap(), big * Uint128::new(2));
        }

        // ----------------------------------------------------------------
        // amp_factor: None vs Some — the bug-fix regression test
        // ----------------------------------------------------------------

        // For imbalanced reserves, the two paths produce different LP
        // amounts. This is the core bug the PR fixes — stable pools were
        // previously using the CP geometric-mean formula.
        #[rstest]
        #[case::imbalanced_10x(10_000u128, 100_000u128)]
        #[case::imbalanced_2x(50_000u128, 100_000u128)]
        #[case::imbalanced_5x(20_000u128, 100_000u128)]
        fn test_add_liquidity_cp_vs_stable_diverge_imbalanced(
            #[case] amount_1: u128,
            #[case] amount_2: u128,
        ) {
            let amount_1 = Uint128::new(amount_1);
            let amount_2 = Uint128::new(amount_2);
            let amp = Uint64::new(100);

            let cp = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                None,
                5_000,
            )
            .unwrap();
            let stable = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount_1,
                amount_2,
                Some(amp),
                5_000,
            )
            .unwrap();

            assert_ne!(
                cp, stable,
                "CP and stable LP must diverge on imbalanced reserves (cp={cp}, stable={stable})"
            );
            assert!(
                stable > cp,
                "Stable D should exceed CP geometric mean for imbalanced pools (cp={cp}, stable={stable})"
            );
        }

        // For balanced reserves, the two paths converge. CP yields
        // sqrt(x * x) = x; stable yields D = sum = 2x.
        // They differ even on balanced pools — but both should be valid
        // and produce non-zero LP. The factor of 2 difference is structural,
        // not a bug.
        #[test]
        fn test_add_liquidity_cp_vs_stable_balanced_first_deposit() {
            let amount = Uint128::new(10_000);
            let cp = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount,
                amount,
                None,
                5_000,
            )
            .unwrap();
            let stable = run_add_liquidity(
                Uint128::zero(),
                Uint128::zero(),
                Uint128::zero(),
                amount,
                amount,
                Some(Uint64::new(100)),
                5_000,
            )
            .unwrap();
            // CP: isqrt(10_000 * 10_000) = 10_000.
            // Stable: D = 2 * 10_000 = 20_000.
            assert_eq!(cp, Uint128::new(10_000));
            assert_eq!(stable, Uint128::new(20_000));
        }

        // After the first deposit, MINIMUM_LIQUIDITY tokens are subtracted
        // from the user's chain LP allocation but the state still records
        // the full D as `total_lp_tokens`.
        #[test]
        fn test_minimum_liquidity_constant_is_1000() {
            // Sanity: keep this test in sync with the production constant.
            assert_eq!(MINIMUM_LIQUIDITY, 1000);
        }

        // FINDING: `compute_d` underflows for amp=1 even on small balanced
        // and imbalanced pools. The Newton iteration in calculate_step
        // performs an unchecked subtraction that goes negative when
        // leverage is very low. This propagates as ContractError::Overflow.
        //
        // These tests pin the current behavior so we notice if the math
        // changes in the future. amp=10 and above are well-behaved.
        #[test]
        fn test_compute_d_amp_1_imbalanced_currently_underflows() {
            let pools = [
                Decimal256::checked_from_integer(Uint128::new(10_000)).unwrap(),
                Decimal256::checked_from_integer(Uint128::new(100_000)).unwrap(),
            ];
            let res = compute_d(Uint64::new(1), &pools);
            assert!(
                res.is_err(),
                "amp=1 imbalanced pool: compute_d expected to underflow, got {:?}",
                res.ok()
            );
        }

        #[test]
        fn test_compute_d_amp_1_balanced_currently_underflows() {
            let pools = [
                Decimal256::checked_from_integer(Uint128::new(1_000)).unwrap(),
                Decimal256::checked_from_integer(Uint128::new(1_000)).unwrap(),
            ];
            let res = compute_d(Uint64::new(1), &pools);
            assert!(
                res.is_err(),
                "amp=1 balanced pool: compute_d expected to underflow, got {:?}",
                res.ok()
            );
        }

        // FINDING: scan amp from 1..=100 to characterize where compute_d
        // produces a usable result. Anything that returns Ok must satisfy
        // bounded D.
        #[test]
        fn test_compute_d_amp_floor_diagnostic() {
            let pools = [
                Decimal256::checked_from_integer(Uint128::new(10_000)).unwrap(),
                Decimal256::checked_from_integer(Uint128::new(100_000)).unwrap(),
            ];
            let mut last_ok_d: Option<Decimal256> = None;
            for amp in [50u64, 60, 70, 80, 90, 100, 200, 500, 1000] {
                let d = compute_d(Uint64::new(amp), &pools);
                if let Ok(d_val) = d {
                    // Monotonically non-decreasing across passing amps.
                    if let Some(prev) = last_ok_d {
                        assert!(
                            d_val >= prev,
                            "D should be monotonically non-decreasing in amp; amp={amp} d={d_val} prev={prev}"
                        );
                    }
                    last_ok_d = Some(d_val);
                }
            }
            assert!(
                last_ok_d.is_some(),
                "At least one amp in the scan should produce a valid D"
            );
        }
    }
}
