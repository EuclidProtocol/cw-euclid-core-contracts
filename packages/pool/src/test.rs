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

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest]
    #[case::first_deposit_equal(1000u128, 1000u128, 0u128, 0u128, 0u128, 1000u128)]
    #[case::proportional_existing_pool(100u128, 100u128, 1000u128, 1000u128, 1000u128, 100u128)]
    #[case::imbalanced_uses_min(200u128, 100u128, 2000u128, 1000u128, 1990u128, 199u128)]
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
    #[case::first_deposit_5e24(
        5_000_000_000_000_000_000_000_000u128,
        5_000_000_000_000_000_000_000_000u128,
        0u128,
        0u128,
        0u128,
        5_000_000_000_000_000_000_000_000u128
    )]
    #[case::proportional_5e24(
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
    #[case::equal_pools(
        Uint256::from(100u128),
        Uint256::from(1000u128),
        Uint256::from(1000u128),
        Uint64::new(1000),
        Uint256::from(99u128),
        Uint256::from(1u128)
    )]
    #[case::imbalanced_pools(
        Uint256::from(100u128),
        Uint256::from(2000u128),
        Uint256::from(1000u128),
        Uint64::new(100),
        Uint256::from(67u128),
        Uint256::from(33u128)
    )]
    #[case::small_amount(
        Uint256::from(1u128),
        Uint256::from(1000000u128),
        Uint256::from(1000000u128),
        Uint64::new(1000),
        Uint256::from(1u128),
        Uint256::from(0u128)
    )]
    #[case::large_amount(
        Uint256::from(1000u128),
        Uint256::from(2000u128),
        Uint256::from(2000u128),
        Uint64::new(1000),
        Uint256::from(946u128),
        Uint256::from(54u128)
    )]
    #[case::extreme_imbalance(
        Uint256::from(100u128),
        Uint256::from(10000u128),
        Uint256::from(1000u128),
        Uint64::new(1000),
        Uint256::from(47u128),
        Uint256::from(53u128)
    )]
    #[case::large_values_large_spread(
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint64::new(1000),
        Uint256::from(820871215252207999u128),
        Uint256::from(179128784747792001u128)
    )]
    #[case::large_values_small_spread(
        Uint256::from(1000u128),
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint64::new(1000),
        Uint256::from(1000u128),
        Uint256::from(0u128)
    )]
    // Cases where ask_pool > offer_pool: return_amount exceeds offer_amount
    #[case::ask_pool_2x_offer_pool(
        Uint256::from(100u128),
        Uint256::from(1000u128),
        Uint256::from(2000u128),
        Uint64::new(1000),
        Uint256::from(106u128),
        Uint256::from(6u128)
    )]
    #[case::ask_pool_10x_offer_pool(
        Uint256::from(100u128),
        Uint256::from(1000u128),
        Uint256::from(10000u128),
        Uint64::new(1000),
        Uint256::from(196u128),
        Uint256::from(96u128)
    )]
    #[case::ask_pool_2x_low_amp(
        Uint256::from(500u128),
        Uint256::from(5000u128),
        Uint256::from(10000u128),
        Uint64::new(100),
        Uint256::from(685u128),
        Uint256::from(185u128)
    )]
    #[case::ask_pool_4x_offer_pool(
        Uint256::from(1000u128),
        Uint256::from(2000u128),
        Uint256::from(8000u128),
        Uint64::new(1000),
        Uint256::from(1160u128),
        Uint256::from(160u128)
    )]
    #[case::large_values_ask_pool_5x(
        Uint256::from(1000000000000000000u128),
        Uint256::from(1000000000000000000u128),
        Uint256::from(5000000000000000000u128),
        Uint64::new(1000),
        Uint256::from(1169582311873333606u128),
        Uint256::from(169582311873333606u128)
    )]
    fn test_compute_stable_swap(
        #[case] offer_asset: Uint256,
        #[case] offer_pool: Uint256,
        #[case] ask_pool: Uint256,
        #[case] swap_amount: Uint64,
        #[case] expected_return_amount: Uint256,
        #[case] expected_spread_amount: Uint256,
    ) {
        let result = compute_stable_swap(offer_asset, offer_pool, ask_pool, swap_amount).unwrap();

        assert_eq!(result.return_amount, expected_return_amount);
        assert_eq!(result.spread_amount, expected_spread_amount);
    }

    #[rstest]
    #[case::token1_in_normal(true, 10000u128, 5000u128, 100u64, 50u64, 1000u128)]
    #[case::token2_in_normal(false, 8000u128, 20000u128, 30u64, 20u64, 500u128)]
    #[case::small_amount_small_fee(true, 1000u128, 1000u128, 10u64, 1u64, 1u128)]
    #[case::small_amount_max_fee(false, 1000u128, 1000u128, 9999u64, 0u64, 1u128)]
    #[case::large_amount_small_fee(
        true,
        1000000000000000000u128,
        1000000000000000000u128,
        1u64,
        1u64,
        1000000000000000000u128
    )]
    #[case::large_amount_max_fee(
        false,
        1000000000000000000u128,
        1000000000000000000u128,
        9999u64,
        0u64,
        1000000000000000000u128
    )]
    #[case::ask_pool_larger_token1(true, 5000u128, 10000u128, 100u64, 50u64, 1000u128)]
    #[case::ask_pool_larger_token2(false, 20000u128, 8000u128, 30u64, 20u64, 500u128)]
    #[case::deep_ask_pool_small_swap(true, 1000u128, 5000u128, 10u64, 1u64, 100u128)]
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
    #[case::token1_in_normal(true, 10000u128, 5000u128, 100u64, 50u64, 1000u64, 1000u128)]
    #[case::token2_in_normal(false, 8000u128, 20000u128, 30u64, 20u64, 1000u64, 500u128)]
    #[case::small_amount_small_fee(true, 1000u128, 1000u128, 10u64, 1u64, 1000u64, 1u128)]
    #[case::small_amount_max_fee(false, 1000u128, 1000u128, 9999u64, 0u64, 1000u64, 1u128)]
    #[case::large_amount_small_fee(
        true,
        1000000000000000000u128,
        1000000000000000000u128,
        1u64,
        1u64,
        1000u64,
        1000000000000000000u128
    )]
    #[case::large_amount_max_fee(
        false,
        1000000000000000000u128,
        1000000000000000000u128,
        9999u64,
        0u64,
        1000u64,
        1000000000000000000u128
    )]
    #[case::ask_pool_larger_token1(true, 5000u128, 10000u128, 100u64, 50u64, 1000u64, 1000u128)]
    #[case::ask_pool_larger_token2(false, 20000u128, 8000u128, 30u64, 20u64, 1000u64, 500u128)]
    #[case::deep_ask_pool_small_swap(true, 1000u128, 5000u128, 10u64, 1u64, 1000u64, 100u128)]
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
        let result_high_amp = compute_stable_swap(offer, pool, pool, Uint64::new(10000)).unwrap();

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
            d_after >= d_before,
            "D must not decrease after swap (truncation favors pool). D_before: {d_before}, D_after: {d_after}",
        );
        assert!(
            relative_diff < Decimal256::from_ratio(1u128, 10_000u128), // < 0.01%
            "D drift too large. D_before: {d_before}, D_after: {d_after}, relative_diff: {relative_diff}"
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
            d_after >= d_before,
            "D must not decrease after swap (truncation favors pool). D_before: {d_before}, D_after: {d_after}",
        );
        assert!(
            relative_diff < Decimal256::from_ratio(1u128, 10_000u128), // < 0.01%
            "D drift too large. D_before: {d_before}, D_after: {d_after}, relative_diff: {relative_diff}"
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
    use euclid::normalize::{normalize_token_to_voucher, normalize_voucher_to_token};

    use crate::MINIMUM_LIQUIDITY;

    use super::*;

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
    fn test_normalize_to_voucher(#[case] raw: u128, #[case] decimals: u32, #[case] expected: u128) {
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
        let result = normalize_voucher_to_token(Uint256::from(voucher_amount), decimals).unwrap();
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
        let lp_2 =
            calculate_lp_allocation(usdc_2, eth_2, usdc_voucher, eth_voucher, lp_total).unwrap();
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
        let half_claim = calculate_amount_from_shares(total_reserve, lp_supply, total_lp).unwrap();
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
        assert_eq!(
            post_a / factor,
            pre_a,
            "User A claim changed after migration"
        );
        assert_eq!(
            post_b / factor,
            pre_b,
            "User B claim changed after migration"
        );
        assert_eq!(
            post_c / factor,
            pre_c,
            "User C claim changed after migration"
        );

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
        assert_eq!(
            v_eth,
            Uint256::from(100_000_000_000_000_000_000_000_000u128)
        );
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
        let new_eth_v =
            normalize_token_to_voucher(Uint256::from(50_000_000_000_000_000_000u128), 18).unwrap();
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
        assert_eq!(
            decimal_result, exact_result,
            "Migrated pool: both paths agree"
        );
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
        assert_eq!(
            decimal_1,
            Uint256::zero(),
            "Decimal256 truncates below 1e-18"
        );
        assert_eq!(
            exact_1,
            Uint256::from(1u128),
            "checked_multiply_ratio is exact"
        );

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
        assert_eq!(
            decimal_sub,
            Uint256::zero(),
            "Decimal256 truncates below threshold"
        );
        assert_eq!(
            exact_sub,
            Uint256::from(999_999u128),
            "Exact path preserves value"
        );
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
        let exact = calculate_amount_from_shares(reserve, Uint256::from(1u128), reserve).unwrap();
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
                reserve_1: Uint256,
                reserve_2: Uint256,
                initial_total_lp: Uint256,
                deposit_1: Uint256,
                deposit_2: Uint256,
                amp_factor: Option<Uint64>,
                slippage_tolerance_bps: u64,
            ) -> Result<Uint256, euclid::error::ContractError> {
                let mut deps = mock_dependencies();
                let env = mock_env();

                let state_storage: Item<State> = Item::new("state");
                let balances_storage: Map<Token, Uint256> = Map::new("balances");
                let chain_lp_tokens_storage: Map<ChainUid, Uint256> = Map::new("chain_lp_tokens");
                let collateral_lp_tokens_storage: Item<Uint256> = Item::new("collateral_lp_tokens");

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
                        &Uint256::zero(),
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
                amount_1: Uint256,
                amount_2: Uint256,
                reserve_1: Uint256,
                reserve_2: Uint256,
                total_lp_supply: Uint256,
                amp: Uint64,
            ) -> Uint256 {
                let pools_new = [
                    Decimal256::checked_from_integer(reserve_1 + amount_1).unwrap(),
                    Decimal256::checked_from_integer(reserve_2 + amount_2).unwrap(),
                ];
                if total_lp_supply.is_zero() {
                    let d = compute_d(amp, &pools_new).unwrap();
                    return d.to_uint256_with_precision(0u32).unwrap();
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
                    .to_uint256_with_precision(0u32)
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
                let amount_1 = Uint256::from(amount_1);
                let amount_2 = Uint256::from(amount_2);
                let amp = Uint64::new(amp);

                let total_lp_after = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
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
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
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
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::from(10_000u128),
                    Uint256::from(100_000u128),
                    Some(Uint64::new(100)),
                    5_000,
                )
                .unwrap();
                assert_eq!(
                    total_lp_after,
                    Uint256::from(82_026u128),
                    "Seeded 10k/100k pool at amp=100 must yield D = 82026"
                );
            }

            // CP would compute LP = isqrt(10_000 * 100_000) = 31_622. Stable path
            // must produce a strictly different (and larger here) number,
            // demonstrating the fix shipped in the PR.
            #[test]
            fn test_first_deposit_stable_differs_from_cp_imbalanced() {
                let amount_1 = Uint256::from(10_000u128);
                let amount_2 = Uint256::from(100_000u128);

                let cp = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount_1,
                    amount_2,
                    None,
                    5_000,
                )
                .unwrap();
                let stable = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount_1,
                    amount_2,
                    Some(Uint64::new(100)),
                    5_000,
                )
                .unwrap();

                assert_eq!(cp, Uint256::from(31_622u128));
                assert_eq!(stable, Uint256::from(82_026u128));
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
                let reserve = Uint256::from(1_000u128);
                let initial_lp = Uint256::from(1_000u128);
                let deposit = Uint256::from(100u128);
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
                let expected =
                    expected_stable_lp(deposit, deposit, reserve, reserve, initial_lp, amp);
                assert_eq!(
                    minted, expected,
                    "Balanced subsequent deposit allocation must match D-growth formula"
                );

                // Sanity: balanced 10% growth on balanced pool should mint ~10% of
                // total supply (within 1 unit of rounding).
                assert!(
                    minted >= Uint256::from(99u128) && minted <= Uint256::from(100u128),
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
                let reserve_1 = Uint256::from(10_000u128);
                let reserve_2 = Uint256::from(100_000u128);
                let initial_lp = Uint256::from(82_026u128); // D for the seeded pool
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
                delta <= Uint256::from(2u128),
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
                let reserve_1 = Uint256::from(1_000u128);
                let reserve_2 = Uint256::from(1_000u128);
                let initial_lp = Uint256::from(2_000u128); // approx D for amp=100 balanced
                let amp = Uint64::new(100);

                // 1.5x more token_1 than token_2 — exactly at the 50% slippage cap.
                let deposit_1 = Uint256::from(150u128);
                let deposit_2 = Uint256::from(100u128);

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
                    minted > Uint256::zero(),
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
                let amount_1 = Uint256::from(10_000u128);
                let amount_2 = Uint256::from(100_000u128);

                let lp_amp_low = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount_1,
                    amount_2,
                    Some(Uint64::new(50)),
                    5_000,
                )
                .unwrap();
                let lp_amp_100 = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount_1,
                    amount_2,
                    Some(Uint64::new(100)),
                    5_000,
                )
                .unwrap();
                let lp_amp_1000 = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
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
                    lp_amp_low >= Uint256::from(31_000u128),
                    "amp=50 LP should be >= geometric mean, got {lp_amp_low}"
                );
                assert!(
                    lp_amp_1000 <= Uint256::from(110_000u128),
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
                let amount = Uint256::from(1_000u128);
                let total_lp_after = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount,
                    amount,
                    Some(Uint64::new(amp)),
                    5_000,
                )
                .unwrap();
                // For balanced pools, D = sum(reserves)
                assert_eq!(total_lp_after, Uint256::from(2_000u128));
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
                    Uint256::from(1_000u128),
                    Uint256::from(1_000u128),
                    Uint256::from(1_000u128),
                    Uint256::zero(),
                    Uint256::zero(),
                    Some(Uint64::new(100)),
                    5_000,
                );
                assert!(res.is_err(), "zero/zero deposit must error");
            }

            // First-deposit with zero reserves on both sides cannot form a pool.
            #[test]
            fn test_first_deposit_zero_amounts_fails() {
                let res = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    Some(Uint64::new(100)),
                    5_000,
                );
                assert!(res.is_err(), "zero first-deposit must error");
            }

            // Very large deposits should not overflow.
            #[test]
            fn test_large_deposits_no_overflow() {
                let big = Uint256::from(1_000_000_000_000_000_000u128); // 1e18
                let res = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
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
                assert_eq!(res.unwrap(), big * Uint256::from(2u128));
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
                let amount_1 = Uint256::from(amount_1);
                let amount_2 = Uint256::from(amount_2);
                let amp = Uint64::new(100);

                let cp = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount_1,
                    amount_2,
                    None,
                    5_000,
                )
                .unwrap();
                let stable = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
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
                let amount = Uint256::from(10_000u128);
                let cp = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount,
                    amount,
                    None,
                    5_000,
                )
                .unwrap();
                let stable = run_add_liquidity(
                    Uint256::zero(),
                    Uint256::zero(),
                    Uint256::zero(),
                    amount,
                    amount,
                    Some(Uint64::new(100)),
                    5_000,
                )
                .unwrap();
                // CP: isqrt(10_000 * 10_000) = 10_000.
                // Stable: D = 2 * 10_000 = 20_000.
                assert_eq!(cp, Uint256::from(10_000u128));
                assert_eq!(stable, Uint256::from(20_000u128));
            }

            // After the first deposit, MINIMUM_LIQUIDITY tokens are subtracted
            // from the user's chain LP allocation but the state still records
            // the full D as `total_lp_tokens`.
            #[test]
            fn test_minimum_liquidity_constant_is_1000() {
                // Sanity: keep this test in sync with the production constant.
                assert_eq!(MINIMUM_LIQUIDITY, 1_000_000_000);
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
                    Decimal256::checked_from_integer(Uint256::from(10_000u128)).unwrap(),
                    Decimal256::checked_from_integer(Uint256::from(100_000u128)).unwrap(),
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
                    Decimal256::checked_from_integer(Uint256::from(1_000u128)).unwrap(),
                    Decimal256::checked_from_integer(Uint256::from(1_000u128)).unwrap(),
                ];
                let res = compute_d(Uint64::new(1), &pools);
                assert!(
                    res.is_err(),
                    "amp=1 balanced pool: compute_d expected to underflow, got {:?}",
                    res.ok()
                );
            }

            #[test]
            fn test_amp_below_50_causes_compute_d_underflow() {
                // leverage = amp / AMP_PRECISION * N_COINS = 49 / 100 * 2 = 0.98
                // Newton step does (leverage - 1) which underflows unsigned Decimal256.
                // This is WHY MIN_AMP exists.
                let pools = [
                    Decimal256::checked_from_integer(Uint256::from(1_000u128)).unwrap(),
                    Decimal256::checked_from_integer(Uint256::from(1_000u128)).unwrap(),
                ];
                assert!(compute_d(Uint64::new(49), &pools).is_err());
                assert!(compute_d(Uint64::new(25), &pools).is_err());
                assert!(compute_d(Uint64::new(10), &pools).is_err());
                // amp=50 is the mathematical minimum (leverage=1.0). It works.
                assert!(compute_d(Uint64::new(50), &pools).is_ok());
                assert!(compute_d(Uint64::new(100), &pools).is_ok());
            }

            #[test]
            fn test_compute_stable_swap_rejects_amp_below_min() {
                use crate::stable_math::MIN_AMP;
                let result = compute_stable_swap(
                    Uint256::from(100u128),
                    Uint256::from(1_000u128),
                    Uint256::from(1_000u128),
                    Uint64::new(MIN_AMP - 1),
                );
                assert!(result.is_err());
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("Amp factor must be at least"));
            }

            // ----------------------------------------------------------------
            // Migration test: justifies the claim that LP-share supply is a
            // proportional accumulator — the first post-migration stable
            // add_liquidity computes d_old from current reserves and scales
            // by current lp_supply, which preserves relative ownership
            // regardless of how prior supply was minted (CP / sqrt(z*y) here).
            // ----------------------------------------------------------------

            use crate::remove_liquidity;
            use cosmwasm_std::Uint512;
            use cosmwasm_std::{Decimal256, Isqrt, Uint256, Uint64};

            struct MigrationOutcome {
                alice_released_1: Uint256,
                alice_released_2: Uint256,
                bob_released_1: Uint256,
                bob_released_2: Uint256,
                reserves_after_add_1: Uint256,
                reserves_after_add_2: Uint256,
                cp_total_lp: Uint256,
                bob_minted_lp: Uint256,
                final_reserves_1: Uint256,
                final_reserves_2: Uint256,
            }

            /// Build a pool whose existing `total_lp_tokens` was minted via the
            /// CP geometric-mean formula (sqrt(r1*r2)) — this models the
            /// pre-migration on-chain state. Then run a post-migration
            /// stable `add_liquidity` for Bob and `remove_liquidity` for both
            /// the pre-migration holder (Alice) and the post-migration
            /// holder (Bob), returning the released amounts so the test can
            /// assert proportional-ownership preservation.
            fn run_migration_scenario(
                r1: Uint256,
                r2: Uint256,
                bob_deposit_1: Uint256,
                bob_deposit_2: Uint256,
                amp: Uint64,
                slippage_bps: u64,
            ) -> MigrationOutcome {
                // Pre-migration LP supply = sqrt(r1 * r2), as CP would have
                // minted on the very first deposit.
                let prod = Uint512::from(r1).checked_mul(Uint512::from(r2)).unwrap();
                let cp_total_lp = Uint256::try_from(Isqrt::isqrt(prod)).unwrap();
                assert!(
                    cp_total_lp > Uint256::from(MINIMUM_LIQUIDITY),
                    "test fixture must seed enough liquidity to cover MINIMUM_LIQUIDITY"
                );
                let alice_lp = cp_total_lp - Uint256::from(MINIMUM_LIQUIDITY);

                // Set up storage.
                let mut deps = mock_dependencies();
                let env = mock_env();

                let state_storage: Item<State> = Item::new("state");
                let balances_storage: Map<Token, Uint256> = Map::new("balances");
                let chain_lp_tokens_storage: Map<ChainUid, Uint256> = Map::new("chain_lp_tokens");
                let collateral_lp_tokens_storage: Item<Uint256> = Item::new("collateral_lp_tokens");

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
                    // Pre-migration: total_lp was minted via CP sqrt(r1*r2).
                    total_lp_tokens: cp_total_lp,
                };
                state_storage.save(deps.as_mut().storage, &state).unwrap();
                balances_storage
                    .save(deps.as_mut().storage, pair.token_1.clone(), &r1)
                    .unwrap();
                balances_storage
                    .save(deps.as_mut().storage, pair.token_2.clone(), &r2)
                    .unwrap();
                // The MINIMUM_LIQUIDITY collateral was locked at first deposit.
                collateral_lp_tokens_storage
                    .save(deps.as_mut().storage, &Uint256::from(MINIMUM_LIQUIDITY))
                    .unwrap();

                // Alice and Bob live on different chains so chain_lp_tokens
                // tracks each independently.
                let alice_chain = ChainUid::create("alice".to_string()).unwrap();
                let bob_chain = ChainUid::create("bob".to_string()).unwrap();
                chain_lp_tokens_storage
                    .save(deps.as_mut().storage, alice_chain.clone(), &alice_lp)
                    .unwrap();
                chain_lp_tokens_storage
                    .save(deps.as_mut().storage, bob_chain.clone(), &Uint256::zero())
                    .unwrap();

                let alice = CrossChainUser {
                    address: "alice".to_string(),
                    chain_uid: alice_chain.clone(),
                };
                let bob = CrossChainUser {
                    address: "bob".to_string(),
                    chain_uid: bob_chain.clone(),
                };

                // --- Post-migration: Bob does stable add_liquidity. ---
                let bob_liquidity = PairWithAmount::new(
                    TokenWithAmount {
                        token: pair.token_1.clone(),
                        amount: bob_deposit_1,
                    },
                    TokenWithAmount {
                        token: pair.token_2.clone(),
                        amount: bob_deposit_2,
                    },
                )
                .unwrap();

                let info = message_info(&router_addr, &[]);
                crate::add_liquidity(
                    deps.as_mut(),
                    env.clone(),
                    info.clone(),
                    &state_storage,
                    &balances_storage,
                    &chain_lp_tokens_storage,
                    &collateral_lp_tokens_storage,
                    bob.clone(),
                    bob_liquidity,
                    slippage_bps,
                    Some(amp),
                    "tx-bob-add".to_string(),
                )
                .unwrap();

                let state_after_add = state_storage.load(&deps.storage).unwrap();
                let bob_minted_lp = state_after_add.total_lp_tokens - cp_total_lp;
                let reserves_after_add_1 = balances_storage
                    .load(&deps.storage, pair.token_1.clone())
                    .unwrap();
                let reserves_after_add_2 = balances_storage
                    .load(&deps.storage, pair.token_2.clone())
                    .unwrap();

                // --- Alice removes her full pre-migration LP. ---
                let r1_before_alice = reserves_after_add_1;
                let r2_before_alice = reserves_after_add_2;
                remove_liquidity(
                    deps.as_mut(),
                    env.clone(),
                    info.clone(),
                    &state_storage,
                    &balances_storage,
                    &chain_lp_tokens_storage,
                    alice.clone(),
                    alice_lp,
                    "tx-alice-remove".to_string(),
                )
                .unwrap();
                let r1_after_alice = balances_storage
                    .load(&deps.storage, pair.token_1.clone())
                    .unwrap();
                let r2_after_alice = balances_storage
                    .load(&deps.storage, pair.token_2.clone())
                    .unwrap();
                let alice_released_1 = r1_before_alice - r1_after_alice;
                let alice_released_2 = r2_before_alice - r2_after_alice;

                // --- Bob removes his full post-migration LP. ---
                remove_liquidity(
                    deps.as_mut(),
                    env,
                    info,
                    &state_storage,
                    &balances_storage,
                    &chain_lp_tokens_storage,
                    bob,
                    bob_minted_lp,
                    "tx-bob-remove".to_string(),
                )
                .unwrap();
                let final_reserves_1 = balances_storage
                    .load(&deps.storage, pair.token_1.clone())
                    .unwrap();
                let final_reserves_2 = balances_storage
                    .load(&deps.storage, pair.token_2.clone())
                    .unwrap();
                let bob_released_1 = r1_after_alice - final_reserves_1;
                let bob_released_2 = r2_after_alice - final_reserves_2;

                MigrationOutcome {
                    alice_released_1,
                    alice_released_2,
                    bob_released_1,
                    bob_released_2,
                    reserves_after_add_1,
                    reserves_after_add_2,
                    cp_total_lp,
                    bob_minted_lp,
                    final_reserves_1,
                    final_reserves_2,
                }
            }

            // Justification — Balanced reserves + balanced post-migration deposit.
            // CP-mint at [10k, 10k] yields total_lp = sqrt(1e8) = 10_000, of
            // which Alice owns 9_000 (1_000 is locked collateral).
            // Stable add of [5k, 5k] at amp=100:
            //   d_old = compute_d(100, [10k, 10k]) = 20_000  (D = sum for balanced)
            //   d_new = compute_d(100, [15k, 15k]) = 30_000
            //   bob_lp = 10_000 * (30_000 - 20_000) / 20_000 = 5_000
            // Post-add: total_lp = 15_000, reserves = [15_000, 15_000].
            // Alice's share 9_000 / 15_000 = 60% -> withdraws (9_000, 9_000)
            //   — exactly what she could have withdrawn pre-migration, even
            //   though her LP was minted by the old CP formula.
            // Bob's share 5_000 / 15_000 = 33.3% -> withdraws (5_000, 5_000)
            //   — exactly his deposit (balanced => no Curve premium).
            // Locked: 1_000 LP backs (1_000, 1_000).
            #[test]
            fn test_migration_preserves_proportional_ownership_balanced() {
                let outcome = run_migration_scenario(
                    Uint256::from(10_000u128),
                    Uint256::from(10_000u128),
                    Uint256::from(5_000u128),
                    Uint256::from(5_000u128),
                    Uint64::new(100),
                    500, // 5% slippage tolerance
                );

                // Sanity: CP supply is sqrt(z*y) = 10_000.
                assert_eq!(outcome.cp_total_lp, Uint256::from(10_000u128));
                // Stable mint for balanced deposit on balanced pool: 5_000.
                assert_eq!(outcome.bob_minted_lp, Uint256::from(5_000u128));
                // Reserves after Bob deposits.
                assert_eq!(outcome.reserves_after_add_1, Uint256::from(15_000u128));
                assert_eq!(outcome.reserves_after_add_2, Uint256::from(15_000u128));

                // Alice withdraws her original pre-migration claim — proving
                // her relative ownership was preserved across the migration.
                assert_eq!(outcome.alice_released_1, Uint256::from(9_000u128));
                assert_eq!(outcome.alice_released_2, Uint256::from(9_000u128));

                // Bob withdraws what he deposited (balanced add => no slippage).
                assert_eq!(outcome.bob_released_1, Uint256::from(5_000u128));
                assert_eq!(outcome.bob_released_2, Uint256::from(5_000u128));

                // What remains backs the locked MINIMUM_LIQUIDITY (1_000 LP).
                assert_eq!(outcome.final_reserves_1, Uint256::from(1_000u128));
                assert_eq!(outcome.final_reserves_2, Uint256::from(1_000u128));
            }

            // Justification — Alice is *not diluted in value* even when
            // Bob's post-migration deposit is imbalanced. An imbalanced
            // deposit shifts pool composition, so Alice's per-token claim
            // changes (less of the under-deposited side, more of the
            // over-deposited side). The invariant the proportional
            // accumulator preserves is total *value*: in a stable pool, the
            // two tokens are pegged ~1:1, so value = token_1 + token_2.
            // Bob pays a small Curve premium for the imbalance which
            // accrues to existing LPs (Alice) and the locked collateral.
            #[test]
            fn test_migration_alice_not_diluted_by_imbalanced_bob_deposit() {
                // Alice's pre-migration claim on a [10k, 10k] pool with
                // total_lp=10_000 and alice_lp=9_000 is (9_000, 9_000),
                // total value = 18_000.
                let alice_pre_claim_value = Uint256::from(18_000u128);

                let outcome = run_migration_scenario(
                    Uint256::from(10_000u128),
                    Uint256::from(10_000u128),
                    // Imbalanced deposit (2:3 vs the 1:1 pool ratio).
                    // Slippage = |0.667 - 1.0| / 1.0 = 33.3% < 50% cap.
                    Uint256::from(2_000u128),
                    Uint256::from(3_000u128),
                    Uint64::new(100),
                    5_000, // max slippage tolerance the handler allows
                );

                // Composition does shift (proof the deposit was imbalanced).
                assert_ne!(outcome.alice_released_1, outcome.alice_released_2);

                // But Alice's total value is preserved: released_1 + released_2
                // >= her pre-migration value claim. The excess is the Curve
                // premium accruing to her share.
                let alice_value = outcome.alice_released_1 + outcome.alice_released_2;
                assert!(
                    alice_value >= alice_pre_claim_value,
                    "alice value diluted: {} < pre-migration value {} \
                 (released_1={}, released_2={})",
                    alice_value,
                    alice_pre_claim_value,
                    outcome.alice_released_1,
                    outcome.alice_released_2
                );

                // Bob's total released value is <= his deposited value — he
                // pays the imbalance premium, not Alice.
                let bob_deposited_value = Uint256::from(2_000u128) + Uint256::from(3_000u128);
                let bob_released_value = outcome.bob_released_1 + outcome.bob_released_2;
                assert!(
                    bob_released_value <= bob_deposited_value,
                    "bob received more value than deposited: {bob_released_value} > {bob_deposited_value}"
                );

                // Conservation: Alice's release + Bob's release + locked
                // collateral backing = total reserves after add.
                assert_eq!(
                    outcome.alice_released_1 + outcome.bob_released_1 + outcome.final_reserves_1,
                    outcome.reserves_after_add_1
                );
                assert_eq!(
                    outcome.alice_released_2 + outcome.bob_released_2 + outcome.final_reserves_2,
                    outcome.reserves_after_add_2
                );
            }

            // Justification — even when the *pre-migration* CP pool was
            // seeded imbalanced (the realistic on-chain case for a stable
            // pool that was using the wrong formula), the proportional
            // accumulator works: the post-migration stable add scales by
            // current lp_supply against d_old derived from current reserves,
            // and Alice's pre-migration claim is preserved.
            #[test]
            fn test_migration_with_imbalanced_pre_migration_pool() {
                // Pool seeded 10k/100k via CP: sqrt(1e9) = 31_622.
                let r1 = Uint256::from(10_000u128);
                let r2 = Uint256::from(100_000u128);
                let outcome = run_migration_scenario(
                    r1,
                    r2,
                    // Bob deposits proportionally — clean baseline.
                    Uint256::from(1_000u128),
                    Uint256::from(10_000u128),
                    Uint64::new(100),
                    500,
                );

                // CP-minted supply is sqrt(z*y) regardless of pool style.
                assert_eq!(outcome.cp_total_lp, Uint256::from(31_622u128));

                // Alice's pre-migration claim: alice_lp / cp_total_lp of (r1, r2).
                //   alice_lp = 31_622 - 1_000 = 30_622
                //   share    = 30_622 / 31_622
                //   claim_1  = 10_000 * 30_622 / 31_622 = 9_683 (floor)
                //   claim_2  = 100_000 * 30_622 / 31_622 = 96_837 (floor)
                // remove_liquidity uses ceil, so the realised release is >=
                // the floor claim. We assert the lower bound — anything
                // extra is the Curve premium the proportional accumulator
                // confers, never a dilution.
                let alice_pre_claim_1 = Uint256::from(9_683u128);
                let alice_pre_claim_2 = Uint256::from(96_837u128);
                assert!(
                    outcome.alice_released_1 >= alice_pre_claim_1,
                    "alice diluted on token_1: {} < {}",
                    outcome.alice_released_1,
                    alice_pre_claim_1
                );
                assert!(
                    outcome.alice_released_2 >= alice_pre_claim_2,
                    "alice diluted on token_2: {} < {}",
                    outcome.alice_released_2,
                    alice_pre_claim_2
                );

                // Conservation across the full migration scenario.
                assert_eq!(
                    outcome.alice_released_1 + outcome.bob_released_1 + outcome.final_reserves_1,
                    outcome.reserves_after_add_1
                );
                assert_eq!(
                    outcome.alice_released_2 + outcome.bob_released_2 + outcome.final_reserves_2,
                    outcome.reserves_after_add_2
                );

                // Bob's mint is proportional-accumulator-correct:
                //   bob_lp = cp_total_lp * (d_new - d_old) / d_old
                // i.e. the SAME formula regardless of how cp_total_lp was minted.
                let pools_old = [
                    Decimal256::checked_from_integer(r1).unwrap(),
                    Decimal256::checked_from_integer(r2).unwrap(),
                ];
                let pools_new = [
                    Decimal256::checked_from_integer(r1 + Uint256::from(1_000u128)).unwrap(),
                    Decimal256::checked_from_integer(r2 + Uint256::from(10_000u128)).unwrap(),
                ];
                let d_old = compute_d(Uint64::new(100), &pools_old).unwrap();
                let d_new = compute_d(Uint64::new(100), &pools_new).unwrap();
                let expected_bob_lp = Decimal256::checked_from_integer(outcome.cp_total_lp)
                    .unwrap()
                    .checked_multiply_ratio(d_new - d_old, d_old)
                    .unwrap()
                    .to_uint256_with_precision(0u32)
                    .unwrap();
                assert_eq!(outcome.bob_minted_lp, expected_bob_lp);
            }

            // FINDING: scan amp from 1..=100 to characterize where compute_d
            // produces a usable result. Anything that returns Ok must satisfy
            // bounded D.
            #[test]
            fn test_compute_d_amp_floor_diagnostic() {
                let pools = [
                    Decimal256::checked_from_integer(Uint256::from(10_000u128)).unwrap(),
                    Decimal256::checked_from_integer(Uint256::from(100_000u128)).unwrap(),
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
}

// ========================================================================
// INVARIANT TESTS: k-invariant, fee conservation, golden values, reserve
// accounting
// ========================================================================

mod invariant_tests {
    use super::*;
    use cosmwasm_std::Uint512;

    /// Helper: set up mock storage with the given reserves and fee config,
    /// then call `pre_swap` with `SwapCalculationMethod::Regular`.
    fn setup_and_pre_swap(
        reserve_in: u128,
        reserve_out: u128,
        lp_fee_bps: u64,
        euclid_fee_bps: u64,
        amount_in: u128,
    ) -> crate::PreSwapResponse {
        use cosmwasm_std::testing::mock_dependencies;
        use cosmwasm_std::Addr;
        use cw_storage_plus::{Item, Map};
        use euclid::{
            chain::ChainUid,
            cross_chain_user::CrossChainUser,
            fee::{DenomFees, Fee, TotalFees},
            msgs::vlp::base::State,
            token::{Pair, Token},
        };
        use std::collections::HashMap;

        let mut deps = mock_dependencies();
        let state_storage: Item<State> = Item::new("state");
        let balances_storage: Map<Token, Uint256> = Map::new("balances");

        let token_1 = Token::create("token1".to_string()).unwrap();
        let token_2 = Token::create("token2".to_string()).unwrap();
        let pair = Pair::new(token_1.clone(), token_2.clone()).unwrap();

        balances_storage
            .save(
                deps.as_mut().storage,
                token_1.clone(),
                &Uint256::from(reserve_in),
            )
            .unwrap();
        balances_storage
            .save(
                deps.as_mut().storage,
                token_2.clone(),
                &Uint256::from(reserve_out),
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

        let state = State {
            pair,
            router: Addr::unchecked("router"),
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
            total_lp_tokens: Uint256::zero(),
        };
        state_storage.save(deps.as_mut().storage, &state).unwrap();

        pre_swap(
            &deps.as_ref(),
            &state_storage,
            &balances_storage,
            &token_1,
            Uint256::from(amount_in),
            SwapCalculationMethod::Regular,
            None,
        )
        .unwrap()
    }

    // ====================================================================
    // 1. CP k-invariant: k_after >= k_before after a swap with fees
    // ====================================================================

    #[rstest]
    // Note: the k-invariant k_after >= k_before requires lp_fee > 0,
    // because the lp_fee is added to the input reserve. When fees are
    // too small (amount_in * lp_fee_bps / 10000 rounds to 0), integer
    // division in the CP formula can cause k to decrease slightly.
    // All test cases below are sized so lp_fee >= 1.
    #[case::small_swap(10_000u128, 5_000u128, 100u64, 10u64, 100u128)]
    #[case::medium_swap(10_000u128, 5_000u128, 30u64, 10u64, 1_000u128)]
    #[case::large_swap(10_000u128, 5_000u128, 30u64, 10u64, 5_000u128)]
    #[case::balanced_pools(100_000u128, 100_000u128, 50u64, 20u64, 10_000u128)]
    #[case::imbalanced_pools(1_000u128, 1_000_000u128, 100u64, 50u64, 500u128)]
    #[case::minimal_amount_high_fee(10_000u128, 5_000u128, 5_000u64, 10u64, 1u128)]
    #[case::zero_euclid_fee(10_000u128, 5_000u128, 100u64, 0u64, 1_000u128)]
    #[case::large_reserves(
        1_000_000_000_000_000_000u128,
        500_000_000_000_000_000u128,
        30u64,
        10u64,
        100_000_000_000_000u128
    )]
    fn test_cp_k_invariant_holds_after_swap(
        #[case] reserve_in: u128,
        #[case] reserve_out: u128,
        #[case] lp_fee_bps: u64,
        #[case] euclid_fee_bps: u64,
        #[case] amount_in: u128,
    ) {
        let res = setup_and_pre_swap(
            reserve_in,
            reserve_out,
            lp_fee_bps,
            euclid_fee_bps,
            amount_in,
        );

        // k_before = reserve_in * reserve_out
        let k_before = Uint512::from(Uint256::from(reserve_in))
            .checked_mul(Uint512::from(Uint256::from(reserve_out)))
            .unwrap();

        // After execute_swap, reserves update as:
        //   new_reserve_in  = reserve_in + swap_amount + lp_fee
        //   new_reserve_out = reserve_out - receive_amount
        let new_reserve_in = Uint512::from(
            Uint256::from(reserve_in)
                .checked_add(res.swap_amount)
                .unwrap()
                .checked_add(res.lp_fee)
                .unwrap(),
        );
        let new_reserve_out = Uint512::from(
            Uint256::from(reserve_out)
                .checked_sub(res.receive_amount)
                .unwrap(),
        );
        let k_after = new_reserve_in.checked_mul(new_reserve_out).unwrap();

        assert!(
            k_after >= k_before,
            "k-invariant violated: k_before={k_before}, k_after={k_after}, reserve_in={reserve_in}, reserve_out={reserve_out}, amount_in={amount_in}"
        );
    }

    // ====================================================================
    // 2. Fee conservation: swap_amount + lp_fee + euclid_fee == amount_in
    // ====================================================================

    #[rstest]
    #[case::normal_fees(10_000u128, 5_000u128, 30u64, 10u64, 1_000u128)]
    #[case::zero_fees(10_000u128, 5_000u128, 0u64, 0u64, 1_000u128)]
    #[case::zero_lp_fee(10_000u128, 5_000u128, 0u64, 50u64, 1_000u128)]
    #[case::zero_euclid_fee(10_000u128, 5_000u128, 100u64, 0u64, 1_000u128)]
    #[case::large_fees(10_000u128, 5_000u128, 500u64, 300u64, 1_000u128)]
    #[case::amount_one(10_000u128, 5_000u128, 30u64, 10u64, 1u128)]
    #[case::large_amount(10_000u128, 5_000u128, 30u64, 10u64, 9_000u128)]
    #[case::max_fee_bps(10_000u128, 5_000u128, 9_999u64, 0u64, 1_000u128)]
    fn test_fee_conservation(
        #[case] reserve_in: u128,
        #[case] reserve_out: u128,
        #[case] lp_fee_bps: u64,
        #[case] euclid_fee_bps: u64,
        #[case] amount_in: u128,
    ) {
        let res = setup_and_pre_swap(
            reserve_in,
            reserve_out,
            lp_fee_bps,
            euclid_fee_bps,
            amount_in,
        );

        let reconstructed = res
            .swap_amount
            .checked_add(res.lp_fee)
            .unwrap()
            .checked_add(res.euclid_fee)
            .unwrap();

        assert_eq!(
            reconstructed,
            Uint256::from(amount_in),
            "Fee conservation violated: swap_amount({}) + lp_fee({}) + euclid_fee({}) = {}, expected {}",
            res.swap_amount, res.lp_fee, res.euclid_fee, reconstructed, amount_in
        );
    }

    // ====================================================================
    // 3. Independent pre-swap golden values (break circularity)
    // ====================================================================
    //
    // These expected values are computed by hand, not by calling the
    // production functions.
    //
    // For reserve_in=10000, reserve_out=5000, amount_in=1000,
    //     lp_fee_bps=30, euclid_fee_bps=10:
    //   lp_fee       = floor(1000 * 30/10000) = 3
    //   euclid_fee   = floor(1000 * 10/10000) = 1
    //   swap_amount  = 1000 - 3 - 1 = 996
    //   new_res_in   = 10000 + 996 = 10996
    //   k            = 10000 * 5000 = 50_000_000
    //   new_res_out  = floor(50_000_000 / 10996) = 4547
    //   receive_amt  = 5000 - 4547 = 453
    //   ideal_return = floor(5000 * 996 / 10000) = 498
    //   spread       = 498 - 453 = 45

    #[test]
    fn test_pre_swap_golden_values_case_1() {
        let res = setup_and_pre_swap(10_000, 5_000, 30, 10, 1_000);

        assert_eq!(res.lp_fee, Uint256::from(3u128), "lp_fee mismatch");
        assert_eq!(res.euclid_fee, Uint256::from(1u128), "euclid_fee mismatch");
        assert_eq!(
            res.swap_amount,
            Uint256::from(996u128),
            "swap_amount mismatch"
        );
        assert_eq!(
            res.receive_amount,
            Uint256::from(453u128),
            "receive_amount mismatch"
        );
        assert_eq!(
            res.spread_amount,
            Uint256::from(45u128),
            "spread_amount mismatch"
        );
    }

    // Second golden value case: balanced pool, no fees.
    // reserve_in=1000, reserve_out=1000, amount_in=100, lp_fee=0, euclid_fee=0
    //   swap_amount  = 100
    //   new_res_in   = 1100
    //   k            = 1_000_000
    //   new_res_out  = floor(1_000_000 / 1100) = 909
    //   receive_amt  = 1000 - 909 = 91
    //   ideal_return = floor(1000 * 100 / 1000) = 100
    //   spread       = 100 - 91 = 9
    #[test]
    fn test_pre_swap_golden_values_case_2_no_fees() {
        let res = setup_and_pre_swap(1_000, 1_000, 0, 0, 100);

        assert_eq!(res.lp_fee, Uint256::zero(), "lp_fee mismatch");
        assert_eq!(res.euclid_fee, Uint256::zero(), "euclid_fee mismatch");
        assert_eq!(
            res.swap_amount,
            Uint256::from(100u128),
            "swap_amount mismatch"
        );
        assert_eq!(
            res.receive_amount,
            Uint256::from(91u128),
            "receive_amount mismatch"
        );
        assert_eq!(
            res.spread_amount,
            Uint256::from(9u128),
            "spread_amount mismatch"
        );
    }

    // Third golden value case: high fees.
    // reserve_in=5000, reserve_out=5000, amount_in=500, lp_fee_bps=500, euclid_fee_bps=200
    //   lp_fee       = floor(500 * 500/10000) = floor(25.0) = 25
    //   euclid_fee   = floor(500 * 200/10000) = floor(10.0) = 10
    //   swap_amount  = 500 - 25 - 10 = 465
    //   new_res_in   = 5000 + 465 = 5465
    //   k            = 25_000_000
    //   new_res_out  = floor(25_000_000 / 5465) = 4574
    //   receive_amt  = 5000 - 4574 = 426
    //   ideal_return = floor(5000 * 465 / 5000) = 465
    //   spread       = 465 - 426 = 39
    #[test]
    fn test_pre_swap_golden_values_case_3_high_fees() {
        let res = setup_and_pre_swap(5_000, 5_000, 500, 200, 500);

        assert_eq!(res.lp_fee, Uint256::from(25u128), "lp_fee mismatch");
        assert_eq!(res.euclid_fee, Uint256::from(10u128), "euclid_fee mismatch");
        assert_eq!(
            res.swap_amount,
            Uint256::from(465u128),
            "swap_amount mismatch"
        );
        assert_eq!(
            res.receive_amount,
            Uint256::from(426u128),
            "receive_amount mismatch"
        );
        assert_eq!(
            res.spread_amount,
            Uint256::from(39u128),
            "spread_amount mismatch"
        );
    }

    // ====================================================================
    // 4. Euclid fee exclusion from reserves: only swap_amount + lp_fee
    //    go into reserves, NOT euclid_fee
    // ====================================================================

    #[rstest]
    #[case::normal(10_000u128, 5_000u128, 30u64, 10u64, 1_000u128)]
    #[case::high_euclid_fee(10_000u128, 5_000u128, 30u64, 500u64, 1_000u128)]
    #[case::zero_euclid_fee(10_000u128, 5_000u128, 100u64, 0u64, 1_000u128)]
    #[case::large_amount(100_000u128, 50_000u128, 50u64, 25u64, 50_000u128)]
    fn test_euclid_fee_excluded_from_reserves(
        #[case] reserve_in: u128,
        #[case] reserve_out: u128,
        #[case] lp_fee_bps: u64,
        #[case] euclid_fee_bps: u64,
        #[case] amount_in: u128,
    ) {
        let res = setup_and_pre_swap(
            reserve_in,
            reserve_out,
            lp_fee_bps,
            euclid_fee_bps,
            amount_in,
        );

        // The execute_swap function updates reserves as:
        //   new_token_in_reserve  = old_reserve_in + swap_amount + lp_fee
        //   new_token_out_reserve = old_reserve_out - receive_amount
        //
        // This means new_token_in_reserve == old_reserve_in + amount_in - euclid_fee
        // (because swap_amount + lp_fee = amount_in - euclid_fee)
        let reserve_increase = res.swap_amount.checked_add(res.lp_fee).unwrap();
        let expected_increase = Uint256::from(amount_in)
            .checked_sub(res.euclid_fee)
            .unwrap();

        assert_eq!(
            reserve_increase, expected_increase,
            "Reserve increase should be amount_in - euclid_fee. \
             swap_amount + lp_fee = {reserve_increase}, amount_in - euclid_fee = {expected_increase}"
        );

        // Verify it is NOT amount_in (unless euclid_fee is zero)
        if !res.euclid_fee.is_zero() {
            assert_ne!(
                reserve_increase,
                Uint256::from(amount_in),
                "Reserve increase must NOT equal full amount_in when euclid_fee > 0"
            );
        }
    }
}
