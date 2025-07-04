#[cfg(test)]
mod tests {
    use crate::pool::{calculate_amount_from_shares, calculate_lp_allocation};
    use cosmwasm_std::Uint128;

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
}
