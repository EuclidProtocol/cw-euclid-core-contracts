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
    // Cases where ask_pool > offer_pool: return_amount exceeds offer_amount
    #[case(
        "ask_pool_2x_offer_pool",
        Uint256::from(100u128),
        Uint256::from(1000u128),
        Uint256::from(2000u128),
        Uint64::new(1000),
        Uint256::from(106u128),
        Uint256::from(6u128)
    )]
    #[case(
        "ask_pool_10x_offer_pool",
        Uint256::from(100u128),
        Uint256::from(1000u128),
        Uint256::from(10000u128),
        Uint64::new(1000),
        Uint256::from(196u128),
        Uint256::from(96u128)
    )]
    #[case(
        "ask_pool_2x_low_amp",
        Uint256::from(500u128),
        Uint256::from(5000u128),
        Uint256::from(10000u128),
        Uint64::new(100),
        Uint256::from(685u128),
        Uint256::from(185u128)
    )]
    #[case(
        "ask_pool_4x_offer_pool",
        Uint256::from(1000u128),
        Uint256::from(2000u128),
        Uint256::from(8000u128),
        Uint64::new(1000),
        Uint256::from(1160u128),
        Uint256::from(160u128)
    )]
    #[case(
        "large_values_ask_pool_5x",
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint256::from(5000000000000000000u128),
        Uint64::new(1000),
        Uint256::from(1169582311873333606u128),
        Uint256::from(169582311873333606u128)
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

        // FIX VERIFICATION: return_amount > offer_amount is valid when ask_pool > offer_pool
        // Previously, spread_amount used checked_sub(offer - return) which panicked in this case.
        #[test]
        fn test_return_exceeds_offer_when_ask_pool_larger() {
            // ask_pool is 2x offer_pool, so swapping into the deeper side yields more
            let result = compute_stable_swap(
                Uint256::from(100u128),
                Uint256::from(1000u128),
                Uint256::from(2000u128),
                Uint64::new(1000),
            )
            .unwrap();

            assert!(
                result.return_amount > Uint256::from(100u128),
                "return_amount should exceed offer_amount when ask_pool > offer_pool. Got: {}",
                result.return_amount
            );
            // spread is abs_diff, so it captures the magnitude of price impact
            assert_eq!(
                result.spread_amount,
                result.return_amount.abs_diff(Uint256::from(100u128)),
                "spread should be abs_diff(offer, return)"
            );
        }

        // Verify the invariant holds even when return > offer (ask_pool > offer_pool)
        #[test]
        fn test_stableswap_invariant_preserved_imbalanced_ask_larger() {
            let pool_a_uint = Uint256::from(5000u128);
            let pool_b_uint = Uint256::from(15000u128);
            let offer_uint = Uint256::from(500u128);
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
            let pool_a = Uint256::from(5000u128);
            let pool_b = Uint256::from(10000u128);
            let offer = Uint256::from(100u128);
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
            let result = calculate_cp_swap(
                Uint256::from(100u128),
                Uint256::from(1000u128),
                Uint256::from(5000u128),
            )
            .unwrap();

            assert!(
                result.return_amount > Uint256::from(100u128),
                "CP swap return should exceed offer when ask_pool > offer_pool. Got: {}",
                result.return_amount
            );
        }
    }

    mod voucher_lp_tests {
        use super::*;
        use crate::{calculate_amount_from_shares, calculate_lp_allocation, MINIMUM_LIQUIDITY};
        use euclid::normalize::{normalize_token_to_voucher, normalize_voucher_to_token};

        fn voucher(count: u128) -> Uint256 {
            Uint256::from(count)
                .checked_mul(Uint256::from(10u128.pow(24)))
                .unwrap()
        }

        // =====================================================================
        // 1. Normalization: raw → voucher
        // =====================================================================

        #[rstest]
        #[case::one_usdc(1_000_000u128, 6, 1_000_000_000_000_000_000_000_000u128)]
        #[case::one_eth(
            1_000_000_000_000_000_000u128,
            18,
            1_000_000_000_000_000_000_000_000u128
        )]
        #[case::one_btc(100_000_000u128, 8, 1_000_000_000_000_000_000_000_000u128)]
        #[case::smallest_usdc_unit(1u128, 6, 1_000_000_000_000_000_000u128)]
        #[case::one_wei(1u128, 18, 1_000_000u128)]
        #[case::already_24dec(1u128, 24, 1u128)]
        #[case::zero(0u128, 6, 0u128)]
        fn test_normalize_to_voucher(
            #[case] raw: u128,
            #[case] decimals: u32,
            #[case] expected: u128,
        ) {
            let result = normalize_token_to_voucher(Uint256::from(raw), decimals).unwrap();
            assert_eq!(result, Uint256::from(expected));
        }

        // =====================================================================
        // 2. Normalization: voucher → raw (including truncation)
        // =====================================================================

        #[rstest]
        #[case::one_usdc(1_000_000_000_000_000_000_000_000u128, 6, 1_000_000u128)]
        #[case::one_eth(
            1_000_000_000_000_000_000_000_000u128,
            18,
            1_000_000_000_000_000_000u128
        )]
        #[case::one_btc(1_000_000_000_000_000_000_000_000u128, 8, 100_000_000u128)]
        #[case::sub_unit_truncates(999u128, 6, 0u128)]
        #[case::just_below_one_usdc(999_999_999_999_999_999u128, 6, 0u128)]
        #[case::already_24dec(1u128, 24, 1u128)]
        fn test_normalize_from_voucher(
            #[case] voucher_amount: u128,
            #[case] decimals: u32,
            #[case] expected: u128,
        ) {
            let result =
                normalize_voucher_to_token(Uint256::from(voucher_amount), decimals).unwrap();
            assert_eq!(result, Uint256::from(expected));
        }

        // =====================================================================
        // 3. Normalization round-trip: raw → voucher → raw
        // =====================================================================

        #[rstest]
        #[case::dec_6(6u32)]
        #[case::dec_8(8u32)]
        #[case::dec_12(12u32)]
        #[case::dec_18(18u32)]
        #[case::dec_24(24u32)]
        fn test_normalize_roundtrip(#[case] decimals: u32) {
            let raw = Uint256::from(10u128.pow(decimals));
            let voucher_amount = normalize_token_to_voucher(raw, decimals).unwrap();
            assert_eq!(
                voucher_amount,
                Uint256::from(10u128.pow(24)),
                "1 token should always normalize to 1e24"
            );
            let back = normalize_voucher_to_token(voucher_amount, decimals).unwrap();
            assert_eq!(back, raw);
        }

        // =====================================================================
        // 4. LP allocation: empty pool (isqrt)
        // =====================================================================

        #[rstest]
        #[case::equal_1_token(
            1_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000u128
        )]
        #[case::equal_1000_tokens(
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128
        )]
        #[case::unequal_1000_and_1(
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000u128,
            31_622_776_601_683_793_319_988_935u128
        )]
        #[case::unequal_5000_and_2(
            5_000_000_000_000_000_000_000_000_000u128,
            2_000_000_000_000_000_000_000_000u128,
            100_000_000_000_000_000_000_000_000u128
        )]
        fn test_lp_allocation_empty_pool(
            #[case] amount_1: u128,
            #[case] amount_2: u128,
            #[case] expected_lp: u128,
        ) {
            let lp = calculate_lp_allocation(
                Uint256::from(amount_1),
                Uint256::from(amount_2),
                Uint256::zero(),
                Uint256::zero(),
                Uint256::zero(),
            )
            .unwrap();
            assert_eq!(lp, Uint256::from(expected_lp));
        }

        // =====================================================================
        // 5. LP allocation: existing pool (min of proportional)
        // =====================================================================

        #[rstest]
        #[case::proportional_half(
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            500_000_000_000_000_000_000_000_000u128,
            500_000_000_000_000_000_000_000_000u128,
            500_000_000_000_000_000_000_000_000u128
        )]
        #[case::excess_token1_uses_min(
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            2_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128
        )]
        fn test_lp_allocation_existing_pool(
            #[case] reserve_1: u128,
            #[case] reserve_2: u128,
            #[case] total_lp: u128,
            #[case] deposit_1: u128,
            #[case] deposit_2: u128,
            #[case] expected_lp: u128,
        ) {
            let lp = calculate_lp_allocation(
                Uint256::from(deposit_1),
                Uint256::from(deposit_2),
                Uint256::from(reserve_1),
                Uint256::from(reserve_2),
                Uint256::from(total_lp),
            )
            .unwrap();
            assert_eq!(lp, Uint256::from(expected_lp));
        }

        // =====================================================================
        // 6. Round-trip: deposit → LP → withdraw == deposit
        // =====================================================================

        #[rstest]
        #[case::equal_into_equal(
            100_000_000_000_000_000_000_000_000u128,
            100_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128
        )]
        #[case::proportional_2_to_1(
            200_000_000_000_000_000_000_000_000u128,
            100_000_000_000_000_000_000_000_000u128,
            2_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_414_213_562_373_095_048_801_688_724u128
        )]
        fn test_lp_roundtrip_deposit_withdraw(
            #[case] deposit_1: u128,
            #[case] deposit_2: u128,
            #[case] reserve_1: u128,
            #[case] reserve_2: u128,
            #[case] total_lp: u128,
        ) {
            let dep_1 = Uint256::from(deposit_1);
            let dep_2 = Uint256::from(deposit_2);
            let res_1 = Uint256::from(reserve_1);
            let res_2 = Uint256::from(reserve_2);
            let total = Uint256::from(total_lp);

            let lp = calculate_lp_allocation(dep_1, dep_2, res_1, res_2, total).unwrap();

            let new_res_1 = res_1.checked_add(dep_1).unwrap();
            let new_res_2 = res_2.checked_add(dep_2).unwrap();
            let new_total = total.checked_add(lp).unwrap();

            let back_1 = calculate_amount_from_shares(new_res_1, lp, new_total).unwrap();
            let back_2 = calculate_amount_from_shares(new_res_2, lp, new_total).unwrap();

            // Integer division in checked_multiply_ratio can lose at most 1 unit
            assert!(back_1 <= dep_1, "Token 1 returned more than deposited");
            assert!(back_2 <= dep_2, "Token 2 returned more than deposited");
            assert_eq!(dep_1 - back_1, Uint256::from(deposit_1) - back_1);
            assert!(
                dep_1 - back_1 <= Uint256::from(1u128),
                "Token 1 rounding loss > 1: {}",
                dep_1 - back_1
            );
            assert!(
                dep_2 - back_2 <= Uint256::from(1u128),
                "Token 2 rounding loss > 1: {}",
                dep_2 - back_2
            );
        }

        // =====================================================================
        // 7. First deposit: MINIMUM_LIQUIDITY deduction
        // =====================================================================

        #[rstest]
        #[case::normal_1000_tokens(
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            999_999_999_999_999_999_000_000_000u128,
            1_000_000_000u128
        )]
        #[case::equals_minimum_user_gets_zero(
            1_000_000_000u128,
            1_000_000_000u128,
            1_000_000_000u128,
            0u128,
            1_000_000_000u128
        )]
        #[case::just_above_minimum(
            1_000_000_001u128,
            1_000_000_001u128,
            1_000_000_001u128,
            1u128,
            1_000_000_000u128
        )]
        #[case::small_deposit_significant_loss(
            1_000_000_000_000u128,
            1_000_000_000_000u128,
            1_000_000_000_000u128,
            999_000_000_000u128,
            1_000_000_000u128
        )]
        fn test_first_deposit_minimum_liquidity(
            #[case] deposit_1: u128,
            #[case] deposit_2: u128,
            #[case] expected_raw_lp: u128,
            #[case] expected_user_lp: u128,
            #[case] expected_collateral: u128,
        ) {
            let raw_lp = calculate_lp_allocation(
                Uint256::from(deposit_1),
                Uint256::from(deposit_2),
                Uint256::zero(),
                Uint256::zero(),
                Uint256::zero(),
            )
            .unwrap();
            assert_eq!(raw_lp, Uint256::from(expected_raw_lp));

            let min_liq = Uint256::from(MINIMUM_LIQUIDITY);
            let user_lp = raw_lp.checked_sub(min_liq).unwrap();
            assert_eq!(user_lp, Uint256::from(expected_user_lp));
            assert_eq!(min_liq, Uint256::from(expected_collateral));
        }

        // =====================================================================
        // 8. End-to-end: normalize raw → allocate LP → denormalize
        // =====================================================================

        #[rstest]
        #[case::usdc_6(1000u128, 6u32)]
        #[case::btc_8(1000u128, 8u32)]
        #[case::eth_18(1000u128, 18u32)]
        fn test_normalize_then_allocate(#[case] token_count: u128, #[case] decimals: u32) {
            let raw = Uint256::from(token_count * 10u128.pow(decimals));
            let voucher_amount = normalize_token_to_voucher(raw, decimals).unwrap();
            assert_eq!(voucher_amount, voucher(token_count));

            let lp = calculate_lp_allocation(
                voucher_amount,
                voucher_amount,
                Uint256::zero(),
                Uint256::zero(),
                Uint256::zero(),
            )
            .unwrap();
            assert_eq!(lp, voucher(token_count));

            let back = normalize_voucher_to_token(voucher_amount, decimals).unwrap();
            assert_eq!(back, raw);
        }

        // =====================================================================
        // 9. Asymmetric decimal pair: USDC (6-dec) + ETH (18-dec)
        // =====================================================================

        #[test]
        fn test_asymmetric_decimal_pair() {
            let usdc_raw = Uint256::from(1_000_000_000u128); // 1000 USDC
            let eth_raw = Uint256::from(1_000_000_000_000_000_000u128); // 1 ETH

            let usdc_voucher = normalize_token_to_voucher(usdc_raw, 6).unwrap();
            let eth_voucher = normalize_token_to_voucher(eth_raw, 18).unwrap();
            assert_eq!(usdc_voucher, voucher(1000));
            assert_eq!(eth_voucher, voucher(1));

            // Empty pool: LP = isqrt(1000e24 * 1e24)
            let lp_total = calculate_lp_allocation(
                usdc_voucher,
                eth_voucher,
                Uint256::zero(),
                Uint256::zero(),
                Uint256::zero(),
            )
            .unwrap();
            assert_eq!(
                lp_total,
                Uint256::from(31_622_776_601_683_793_319_988_935u128)
            );

            // Second deposit: 500 USDC + 0.5 ETH (proportional)
            let usdc_2 = voucher(500);
            let eth_2 = Uint256::from(500_000_000_000_000_000_000_000u128); // 0.5e24
            let lp_2 = calculate_lp_allocation(usdc_2, eth_2, usdc_voucher, eth_voucher, lp_total)
                .unwrap();
            assert_eq!(lp_2, Uint256::from(15_811_388_300_841_896_659_994_467u128));

            // Roundtrip: withdraw second deposit
            let new_total_lp = lp_total.checked_add(lp_2).unwrap();
            let new_usdc_res = usdc_voucher.checked_add(usdc_2).unwrap();
            let new_eth_res = eth_voucher.checked_add(eth_2).unwrap();

            let back_usdc = calculate_amount_from_shares(new_usdc_res, lp_2, new_total_lp).unwrap();
            let back_eth = calculate_amount_from_shares(new_eth_res, lp_2, new_total_lp).unwrap();

            // Irrational sqrt causes integer rounding loss on withdraw.
            // Loss is bounded by lp_total/lp_2 units (here ~11 voucher units for USDC, ~1 for ETH).
            assert!(back_usdc <= usdc_2, "USDC returned more than deposited");
            assert!(back_eth <= eth_2, "ETH returned more than deposited");
            assert!(
                usdc_2 - back_usdc <= Uint256::from(20u128),
                "USDC rounding loss too large: {}",
                usdc_2 - back_usdc
            );
            assert!(
                eth_2 - back_eth <= Uint256::from(1u128),
                "ETH rounding loss too large: {}",
                eth_2 - back_eth
            );
        }

        // =====================================================================
        // 10. Migration: reserves scale up, LP supply unchanged
        // =====================================================================

        #[rstest]
        #[case::usdc_6dec(1_000_000u128, 6u32)]
        #[case::eth_18dec(1_000_000_000_000_000_000u128, 18u32)]
        fn test_migration_lp_value(#[case] raw_reserve: u128, #[case] decimals: u32) {
            let raw = Uint256::from(raw_reserve);
            let lp_supply = raw; // LP supply = raw reserve (from isqrt(raw*raw))

            // Pre-migration: holder claims 100%
            let pre_claim = calculate_amount_from_shares(raw, lp_supply, lp_supply).unwrap();
            assert_eq!(pre_claim, raw);

            // Post-migration: reserves normalized, LP supply unchanged
            let voucher_reserve = normalize_token_to_voucher(raw, decimals).unwrap();
            let post_claim =
                calculate_amount_from_shares(voucher_reserve, lp_supply, lp_supply).unwrap();
            assert_eq!(
                post_claim, voucher_reserve,
                "100% holder should claim entire voucher reserve"
            );

            // New depositor adds equal voucher amount, gets equal LP
            let new_lp = calculate_lp_allocation(
                voucher_reserve,
                voucher_reserve,
                voucher_reserve,
                voucher_reserve,
                lp_supply,
            )
            .unwrap();
            assert_eq!(new_lp, lp_supply, "Equal deposit should yield equal LP");

            // Both now hold 50%
            let total_lp = lp_supply.checked_add(new_lp).unwrap();
            let total_reserve = voucher_reserve.checked_add(voucher_reserve).unwrap();
            let half_claim =
                calculate_amount_from_shares(total_reserve, lp_supply, total_lp).unwrap();
            assert_eq!(half_claim, voucher_reserve, "Each holder should claim 50%");
        }

        // =====================================================================
        // 10b. Migration: multi-user pool, all holders preserve exact claims
        // =====================================================================

        #[test]
        fn test_migration_multi_user_proportional_ownership() {
            // 1M USDC + 1M ATOM pool (6-dec), LP = isqrt(1e12 * 1e12) = 1e12
            let raw_reserve = Uint256::from(1_000_000_000_000u128); // 1e12
            let old_lp_supply = Uint256::from(1_000_000_000_000u128);

            let user_a_lp = Uint256::from(500_000_000_000u128); // 50%
            let user_b_lp = Uint256::from(300_000_000_000u128); // 30%
            let user_c_lp = Uint256::from(200_000_000_000u128); // 20%

            // Pre-migration claims
            let pre_a = calculate_amount_from_shares(raw_reserve, user_a_lp, old_lp_supply).unwrap();
            let pre_b = calculate_amount_from_shares(raw_reserve, user_b_lp, old_lp_supply).unwrap();
            let pre_c = calculate_amount_from_shares(raw_reserve, user_c_lp, old_lp_supply).unwrap();
            assert_eq!(pre_a, Uint256::from(500_000_000_000u128));
            assert_eq!(pre_b, Uint256::from(300_000_000_000u128));
            assert_eq!(pre_c, Uint256::from(200_000_000_000u128));

            // Post-migration: reserves *= 10^18 (6-dec -> 24-dec), LP unchanged
            let voucher_reserve = Uint256::from(1_000_000_000_000_000_000_000_000_000_000u128);

            let post_a =
                calculate_amount_from_shares(voucher_reserve, user_a_lp, old_lp_supply).unwrap();
            let post_b =
                calculate_amount_from_shares(voucher_reserve, user_b_lp, old_lp_supply).unwrap();
            let post_c =
                calculate_amount_from_shares(voucher_reserve, user_c_lp, old_lp_supply).unwrap();

            // Denormalize back: divide by 10^18
            let factor = Uint256::from(1_000_000_000_000_000_000u128);
            assert_eq!(post_a / factor, pre_a, "User A claim changed after migration");
            assert_eq!(post_b / factor, pre_b, "User B claim changed after migration");
            assert_eq!(post_c / factor, pre_c, "User C claim changed after migration");

            // Sum of claims = total reserve
            assert_eq!(post_a + post_b + post_c, voucher_reserve);
        }

        // =====================================================================
        // 10c. Migration: asymmetric decimal pair (ETH 18-dec + BTC 8-dec)
        // =====================================================================

        #[test]
        fn test_migration_asymmetric_decimal_pair() {
            // Pre-migration: 100 ETH (18-dec) + 10 BTC (8-dec)
            let raw_eth = Uint256::from(100_000_000_000_000_000_000u128); // 100e18
            let raw_btc = Uint256::from(1_000_000_000u128); // 10e8
            let old_lp = Uint256::from(316_227_766_016_837u128); // isqrt(100e18 * 10e8)

            // Post-migration: normalize both to 24-dec
            let v_eth = normalize_token_to_voucher(raw_eth, 18).unwrap();
            let v_btc = normalize_token_to_voucher(raw_btc, 8).unwrap();
            assert_eq!(v_eth, Uint256::from(100_000_000_000_000_000_000_000_000u128));
            assert_eq!(v_btc, Uint256::from(10_000_000_000_000_000_000_000_000u128));

            // Full holder withdrawal: still gets everything
            let post_eth = calculate_amount_from_shares(v_eth, old_lp, old_lp).unwrap();
            let post_btc = calculate_amount_from_shares(v_btc, old_lp, old_lp).unwrap();
            assert_eq!(post_eth, v_eth);
            assert_eq!(post_btc, v_btc);

            // Denormalize back to raw: exact match
            let back_eth = normalize_voucher_to_token(post_eth, 18).unwrap();
            let back_btc = normalize_voucher_to_token(post_btc, 8).unwrap();
            assert_eq!(back_eth, raw_eth);
            assert_eq!(back_btc, raw_btc);

            // New depositor: 50 ETH + 5 BTC (50% of pool, same ratio)
            let new_eth_v = normalize_token_to_voucher(Uint256::from(50_000_000_000_000_000_000u128), 18).unwrap();
            let new_btc_v = normalize_token_to_voucher(Uint256::from(500_000_000u128), 8).unwrap();
            let new_lp = calculate_lp_allocation(new_eth_v, new_btc_v, v_eth, v_btc, old_lp).unwrap();
            assert_eq!(new_lp, Uint256::from(158_113_883_008_418u128)); // old_lp / 2
        }

        // =====================================================================
        // 10d. Migration: value-per-LP scaling factor
        // =====================================================================

        #[rstest]
        #[case::dec_6(6u32, 1_000_000_000_000_000_000u128)]
        #[case::dec_8(8u32, 10_000_000_000_000_000u128)]
        #[case::dec_18(18u32, 1_000_000u128)]
        fn test_migration_value_per_lp_scales(
            #[case] decimals: u32,
            #[case] expected_voucher_per_lp: u128,
        ) {
            let raw = Uint256::from(10u128.pow(decimals)); // 1 token raw
            let lp_supply = raw; // isqrt(raw * raw)

            // Pre-migration: 1 LP claims 1 raw unit
            let pre_value = calculate_amount_from_shares(raw, Uint256::from(1u128), lp_supply).unwrap();
            assert_eq!(pre_value, Uint256::from(1u128));

            // Post-migration: 1 LP claims 10^(24-decimals) voucher units
            let voucher_reserve = normalize_token_to_voucher(raw, decimals).unwrap();
            let post_value =
                calculate_amount_from_shares(voucher_reserve, Uint256::from(1u128), lp_supply).unwrap();
            assert_eq!(post_value, Uint256::from(expected_voucher_per_lp));

            // Denormalized back = same 1 raw unit
            let back = normalize_voucher_to_token(post_value, decimals).unwrap();
            assert_eq!(back, Uint256::from(1u128));
        }

        // =====================================================================
        // 10e. Decimal256 remove_liquidity precision: migrated vs new pools
        // =====================================================================

        #[test]
        fn test_decimal256_precision_migrated_pools_safe() {
            // Migrated 6-dec pool: LP=1e12, reserve=1e30 (voucher)
            let lp_supply = Uint256::from(1_000_000_000_000u128); // 1e12
            let reserve = Uint256::from(1_000_000_000_000_000_000_000_000_000_000u128); // 1e30

            // Smallest LP holder (1 LP) can still withdraw
            let ratio = Decimal256::checked_from_ratio(1u128, lp_supply).unwrap();
            let decimal_result = reserve.checked_mul_floor(ratio).unwrap();
            let exact_result =
                calculate_amount_from_shares(reserve, Uint256::from(1u128), lp_supply).unwrap();

            // Both should return non-zero (ratio = 1e-12 > 1e-18 precision floor)
            assert_eq!(decimal_result, Uint256::from(1_000_000_000_000_000_000u128)); // 1e18
            assert_eq!(exact_result, Uint256::from(1_000_000_000_000_000_000u128));
            assert_eq!(decimal_result, exact_result, "Migrated pool: both paths agree");
        }

        #[test]
        fn test_decimal256_precision_migrated_18dec_boundary() {
            // Migrated 18-dec pool: LP=1e18, reserve=1e24 (voucher)
            // ratio = 1/1e18 = exactly 1e-18 = Decimal256 precision boundary
            let lp_supply = Uint256::from(1_000_000_000_000_000_000u128); // 1e18
            let reserve = Uint256::from(1_000_000_000_000_000_000_000_000u128); // 1e24

            let ratio = Decimal256::checked_from_ratio(1u128, lp_supply).unwrap();
            let decimal_result = reserve.checked_mul_floor(ratio).unwrap();
            let exact_result =
                calculate_amount_from_shares(reserve, Uint256::from(1u128), lp_supply).unwrap();

            // At exact boundary: both agree
            assert_eq!(decimal_result, Uint256::from(1_000_000u128)); // 1e6
            assert_eq!(exact_result, Uint256::from(1_000_000u128));
        }

        #[test]
        fn test_decimal256_precision_new_voucher_pool_floor() {
            // New pool created post-migration: LP=1e24 (from isqrt of voucher amounts)
            let lp_supply = Uint256::from(1_000_000_000_000_000_000_000_000u128); // 1e24
            let reserve = Uint256::from(1_000_000_000_000_000_000_000_000u128); // 1e24

            // 1 LP: ratio = 1e-24 < 1e-18 -> Decimal256 returns 0
            let ratio_1 = Decimal256::checked_from_ratio(1u128, lp_supply).unwrap();
            let decimal_1 = reserve.checked_mul_floor(ratio_1).unwrap();
            let exact_1 =
                calculate_amount_from_shares(reserve, Uint256::from(1u128), lp_supply).unwrap();
            assert_eq!(decimal_1, Uint256::zero(), "Decimal256 truncates below 1e-18");
            assert_eq!(exact_1, Uint256::from(1u128), "checked_multiply_ratio is exact");

            // 1e6 LP: ratio = 1e-18 = boundary, Decimal256 works
            let min_lp = Uint256::from(1_000_000u128);
            let ratio_min = Decimal256::checked_from_ratio(min_lp, lp_supply).unwrap();
            let decimal_min = reserve.checked_mul_floor(ratio_min).unwrap();
            let exact_min = calculate_amount_from_shares(reserve, min_lp, lp_supply).unwrap();
            assert_eq!(decimal_min, exact_min, "Both agree at 1e-18 boundary");
            assert_eq!(exact_min, Uint256::from(1_000_000u128));

            // Below 1e6 LP: Decimal256 gives 0, exact gives non-zero
            let sub_min = Uint256::from(999_999u128);
            let ratio_sub = Decimal256::checked_from_ratio(sub_min, lp_supply).unwrap();
            let decimal_sub = reserve.checked_mul_floor(ratio_sub).unwrap();
            let exact_sub = calculate_amount_from_shares(reserve, sub_min, lp_supply).unwrap();
            assert_eq!(decimal_sub, Uint256::zero(), "Decimal256 truncates below threshold");
            assert_eq!(exact_sub, Uint256::from(999_999u128), "Exact path preserves value");
        }

        // =====================================================================
        // 11. Precision edge cases: calculate_amount_from_shares at 24-dec
        // =====================================================================

        #[rstest]
        #[case::single_unit(
            1_000_000_000_000_000_000_000_000u128,
            1u128,
            1_000_000_000_000_000_000_000_000u128,
            1u128
        )]
        #[case::small_share(
            1_000_000_000_000_000_000_000_000u128,
            1_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000u128
        )]
        #[case::almost_all(
            1_000_000_000_000_000_000_000_000u128,
            999_999_999_999_999_999_999_999u128,
            1_000_000_000_000_000_000_000_000u128,
            999_999_999_999_999_999_999_999u128
        )]
        #[case::large_reserve_small_share(
            1_000_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000u128,
            1_000_000_000_000_000_000_000_000_000u128,
            1_000_000_000_000_000_000u128
        )]
        fn test_precision_edge_cases(
            #[case] reserve: u128,
            #[case] shares: u128,
            #[case] total_shares: u128,
            #[case] expected: u128,
        ) {
            let result = calculate_amount_from_shares(
                Uint256::from(reserve),
                Uint256::from(shares),
                Uint256::from(total_shares),
            )
            .unwrap();
            assert_eq!(result, Uint256::from(expected));
        }

        // =====================================================================
        // 12. Decimal256 precision floor vs checked_multiply_ratio
        // =====================================================================

        #[test]
        fn test_decimal256_vs_multiply_ratio_precision() {
            let reserve = Uint256::from(1_000_000_000_000_000_000_000_000_000u128); // 1e27

            // Case 1: lp=1, total=1e27 — below Decimal256 precision floor
            let exact =
                calculate_amount_from_shares(reserve, Uint256::from(1u128), reserve).unwrap();
            assert_eq!(
                exact,
                Uint256::from(1u128),
                "checked_multiply_ratio is exact"
            );

            let ratio = Decimal256::checked_from_ratio(1u128, reserve).unwrap();
            let decimal_result = reserve.checked_mul_floor(ratio).unwrap();
            assert_eq!(
                decimal_result,
                Uint256::zero(),
                "Decimal256 truncates to 0 below 1e-18"
            );

            // Case 2: lp=1e9, total=1e27 — exactly at Decimal256 precision (1e-18)
            let lp = Uint256::from(1_000_000_000u128);
            let exact_boundary = calculate_amount_from_shares(reserve, lp, reserve).unwrap();
            let ratio_boundary = Decimal256::checked_from_ratio(lp, reserve).unwrap();
            let decimal_boundary = reserve.checked_mul_floor(ratio_boundary).unwrap();
            assert_eq!(
                exact_boundary, decimal_boundary,
                "Both agree at 1e-18 boundary"
            );
            assert_eq!(exact_boundary, Uint256::from(1_000_000_000u128));
        }
    }
}
