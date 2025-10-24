#[cfg(test)]
mod tests {
    use crate::pool::{calculate_amount_from_shares, calculate_lp_allocation, stable_math};
    use cosmwasm_std::{Uint128, Uint64};

    #[test]
    fn test_calculate_lp_allocation() {
        // Test initial liquidity provision (empty pool)
        let token_1_amount = Uint128::new(1000);
        let token_2_amount = Uint128::new(1000);
        let total_liquidity_1 = Uint128::zero();
        let total_liquidity_2 = Uint128::zero();
        let total_lp_supply = Uint128::zero();

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For initial provision, LP tokens should be sqrt(token1 * token2) - MINIMUM_LIQUIDITY
        assert_eq!(lp_tokens, Uint128::new(1000));

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

        // Test subsequent liquidity provision
        let token_1_amount = Uint128::new(100);
        let token_2_amount = Uint128::new(100);
        let total_liquidity_1 = Uint128::new(1000);
        let total_liquidity_2 = Uint128::new(1000);
        let total_lp_supply = Uint128::new(1000); // From previous provision

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For subsequent provisions, LP tokens should be proportional to liquidity added
        assert_eq!(lp_tokens, Uint128::new(100));

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

        // Test uneven liquidity provision
        let token_1_amount = Uint128::new(200);
        let token_2_amount = Uint128::new(100);
        let total_liquidity_1 = Uint128::new(2000);
        let total_liquidity_2 = Uint128::new(1000);
        let total_lp_supply = Uint128::new(1990);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // Should use the minimum ratio to prevent dilution
        assert_eq!(lp_tokens, Uint128::new(199));

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

    #[test]
    fn test_big_token_amount() {
        // Test with large token amounts
        let token_1_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let token_2_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_liquidity_1 = Uint128::new(0);
        let total_liquidity_2 = Uint128::new(0);
        let total_lp_supply = Uint128::new(0);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For initial provision with large amounts
        assert_eq!(lp_tokens, Uint128::new(5_000_000_000_000_000_000_000_000));

        // Test subsequent large liquidity provision
        let token_1_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let token_2_amount = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_liquidity_1 = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_liquidity_2 = Uint128::new(5_000_000_000_000_000_000_000_000);
        let total_lp_supply = Uint128::new(5_000_000_000_000_000_000_000_000);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        // For subsequent provisions with large amounts
        assert_eq!(lp_tokens, Uint128::new(5_000_000_000_000_000_000_000_000));
    }

    // Stable LP Minting Tests
    #[test]
    fn test_stable_lp_mint_initial_provision() {
        // Test initial liquidity provision for stable pools
        let reserve_1 = Uint128::new(0);
        let reserve_2 = Uint128::new(0);
        let amount_1 = Uint128::new(1000);
        let amount_2 = Uint128::new(1000);
        let total_lp_supply = Uint128::new(0);
        let amp_factor = Uint64::new(100);

        let lp_tokens = stable_math::compute_stable_lp_mint(
            reserve_1,
            reserve_2,
            amount_1,
            amount_2,
            total_lp_supply,
            amp_factor,
        )
        .unwrap();

        // For initial provision, should return D1 (invariant after deposit)
        // With equal amounts and amp=100, D1 should be approximately 2000
        assert!(lp_tokens > Uint128::new(1990));
        assert!(lp_tokens < Uint128::new(2010));
    }

    #[test]
    fn test_stable_lp_mint_subsequent_provision() {
        // Test subsequent liquidity provision for stable pools
        let reserve_1 = Uint128::new(10000);
        let reserve_2 = Uint128::new(10000);
        let amount_1 = Uint128::new(1000);
        let amount_2 = Uint128::new(1000);
        let total_lp_supply = Uint128::new(20000);
        let amp_factor = Uint64::new(100);

        let lp_tokens = stable_math::compute_stable_lp_mint(
            reserve_1,
            reserve_2,
            amount_1,
            amount_2,
            total_lp_supply,
            amp_factor,
        )
        .unwrap();

        // For subsequent provision, should be proportional: total_lp * (D1 - D0) / D0
        // Should be approximately 10% of total LP supply (1000/10000 = 0.1)
        assert!(lp_tokens > Uint128::new(1900));
        assert!(lp_tokens < Uint128::new(2100));
    }

    #[test]
    fn test_stable_lp_mint_unbalanced_provision() {
        // Test unbalanced liquidity provision
        let reserve_1 = Uint128::new(10000);
        let reserve_2 = Uint128::new(10000);
        let amount_1 = Uint128::new(2000);
        let amount_2 = Uint128::new(1000);
        let total_lp_supply = Uint128::new(20000);
        let amp_factor = Uint64::new(100);

        let lp_tokens = stable_math::compute_stable_lp_mint(
            reserve_1,
            reserve_2,
            amount_1,
            amount_2,
            total_lp_supply,
            amp_factor,
        )
        .unwrap();

        // Should handle unbalanced provision correctly
        assert!(lp_tokens > Uint128::new(1000));
        assert!(lp_tokens < Uint128::new(3000));
    }

    #[test]
    fn test_stable_lp_mint_large_amounts() {
        // Test with large amounts
        let reserve_1 = Uint128::new(1_000_000_000);
        let reserve_2 = Uint128::new(1_000_000_000);
        let amount_1 = Uint128::new(100_000_000);
        let amount_2 = Uint128::new(100_000_000);
        let total_lp_supply = Uint128::new(2_000_000_000);
        let amp_factor = Uint64::new(100);

        let lp_tokens = stable_math::compute_stable_lp_mint(
            reserve_1,
            reserve_2,
            amount_1,
            amount_2,
            total_lp_supply,
            amp_factor,
        )
        .unwrap();

        // Should handle large amounts without overflow
        assert!(lp_tokens > Uint128::new(190_000_000));
        assert!(lp_tokens < Uint128::new(210_000_000));
    }

    #[test]
    fn test_stable_lp_mint_different_amp_factors() {
        // Test that different amp factors produce different results with unbalanced deposits
        let reserve_1 = Uint128::new(10000);
        let reserve_2 = Uint128::new(10000);
        let amount_1 = Uint128::new(1000);
        let amount_2 = Uint128::new(500); // Unbalanced deposit to see amp factor difference
        let total_lp_supply = Uint128::new(20000);

        let lp_tokens_low_amp = stable_math::compute_stable_lp_mint(
            reserve_1,
            reserve_2,
            amount_1,
            amount_2,
            total_lp_supply,
            Uint64::new(50), // Low amp factor
        )
        .unwrap();

        let lp_tokens_high_amp = stable_math::compute_stable_lp_mint(
            reserve_1,
            reserve_2,
            amount_1,
            amount_2,
            total_lp_supply,
            Uint64::new(200), // High amp factor
        )
        .unwrap();

        // Both should be valid but different due to unbalanced deposit
        assert!(lp_tokens_low_amp > Uint128::new(1000));
        assert!(lp_tokens_high_amp > Uint128::new(1000));
        assert_ne!(lp_tokens_low_amp, lp_tokens_high_amp);
    }
}
