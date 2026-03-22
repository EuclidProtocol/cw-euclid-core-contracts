#[cfg(test)]
mod tests {
    use crate::{calculate_amount_from_shares, calculate_lp_allocation};
    use cosmwasm_std::{Isqrt, Uint256, Uint512};
    use rstest::rstest;

    #[rstest]
    #[case(1000, 1000, 0, 0, 1000)]
    #[case(100, 100, 1000, 1000, 100)]
    #[case(200, 100, 2000, 1000, 141)]
    #[case(
        5_000_000_000_000_000_000_000_000,
        5_000_000_000_000_000_000_000_000,
        0,
        0,
        5_000_000_000_000_000_000_000_000
    )]
    #[case(
        5_000_000_000_000_000_000_000_000,
        5_000_000_000_000_000_000_000_000,
        5_000_000_000_000_000_000_000_000,
        5_000_000_000_000_000_000_000_000,
        5_000_000_000_000_000_000_000_000
    )]
    #[case(
        10_000_000_000_000_000_000_000,     // token_1_reserve
        100_000_000_000_000_000_000_000,    // token_2_reserve
        0,                                  // total_liquidity_1
        0,                                  // total_liquidity_2
        31_622_776_601_683_793_319_988      // expected_lp_tokens
    )]
    #[case(
        10_000_000_000_000_000_000_000,     // token_1_reserve
        100_000_000_000_000_000_000_000,    // token_2_reserve
        10_000_000_000_000_000_000_000,                                  // total_liquidity_1
        100_000_000_000_000_000_000_000,                                  // total_liquidity_2
        31_622_776_601_683_793_319_988      // expected_lp_tokens
    )]
    #[case(
        1_000_000_000_000_000_000_000_000_000_000_000_000u128,  // very large token_1_reserve (10^36, fits u128)
        1_000_000_000_000_000_000_000_000_000_000_000_000u128,  // very large token_2_reserve
        0u128,                                                 // total_liquidity_1
        0u128,                                                 // total_liquidity_2
        1_000_000_000_000_000_000_000_000_000_000_000_000u128   // expected_lp_tokens == sqrt(token_1 * token_2)
    )]
    #[case(
        12_345_678_987_654_321_000_000_000_000_000_000_000u128,         // another very large token_1_reserve
        98_765_432_123_456_789_000_000_000_000_000_000_000u128,         // another very large token_2_reserve
        12_345_678_987_654_321_000_000_000_000_000_000_000u128,         // total_liquidity_1
        98_765_432_123_456_789_000_000_000_000_000_000_000u128,         // total_liquidity_2
        34_918_853_361_374_275_888_742_362_736_290_181_394u128          // isqrt(token_1_amount * token_2_amount)
    )]
    #[case(
        u128::MAX,
        u128::MAX-8712938791283123123,
        u128::MAX,
        u128::MAX-8712938791283123123,
        340_282_366_920_938_463_459_018_138_036_126_649_893u128
    )]
    #[case(u128::MAX, u128::MAX, u128::MAX, u128::MAX, u128::MAX)]
    fn test_calculate_lp_allocation_on_add_liquidity(
        #[case] token_1_amount: u128,
        #[case] token_2_amount: u128,
        #[case] total_liquidity_1: u128,
        #[case] total_liquidity_2: u128,
        #[case] expected_lp_tokens: u128,
    ) {
        let token_1_amount = Uint256::from(token_1_amount);
        let token_2_amount = Uint256::from(token_2_amount);
        let total_liquidity_1 = Uint256::from(total_liquidity_1);
        let total_liquidity_2 = Uint256::from(total_liquidity_2);
        let total_lp_supply = Uint256::try_from(Isqrt::isqrt(
            Uint512::from(total_liquidity_1)
                .checked_mul(total_liquidity_2.into())
                .unwrap(),
        ))
        .unwrap();
        let expected_lp_tokens = Uint256::from(expected_lp_tokens);

        let lp_tokens = calculate_lp_allocation(
            token_1_amount,
            token_2_amount,
            total_liquidity_1,
            total_liquidity_2,
            total_lp_supply,
        )
        .unwrap();

        assert!(
            lp_tokens.abs_diff(expected_lp_tokens).le(&Uint256::one()),
            "Incorrect LP Tokens {expected_lp_tokens} != {lp_tokens} for token_1_amount {token_1_amount} and token_2_amount {token_2_amount}"
        );

        let amount_1 = calculate_amount_from_shares(
            token_1_amount + total_liquidity_1,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert!(
            amount_1.abs_diff(token_1_amount).le(&Uint256::one()),
            "Token 1 amount from shares is not correct {amount_1} != {token_1_amount}"
        );

        let amount_2 = calculate_amount_from_shares(
            token_2_amount + total_liquidity_2,
            lp_tokens,
            total_lp_supply + lp_tokens,
        )
        .unwrap();
        assert!(
            amount_2.abs_diff(token_2_amount).le(&Uint256::one()),
            "Token 2 amount from shares is not correct {amount_2} != {token_2_amount}"
        );

        let sqrt_lp_tokens = Uint256::try_from(Isqrt::isqrt(
            Uint512::from(total_liquidity_1.checked_add(token_1_amount).unwrap())
                .checked_mul(
                    total_liquidity_2
                        .checked_add(token_2_amount)
                        .unwrap()
                        .into(),
                )
                .unwrap(),
        ))
        .unwrap();

        let expected_total_lp_supply_after_add =
            total_lp_supply.checked_add(expected_lp_tokens).unwrap();
        assert!(
            sqrt_lp_tokens
                .abs_diff(expected_total_lp_supply_after_add)
                .le(&Uint256::one()),
            "Total LP Supply is not correct {expected_total_lp_supply_after_add} != {sqrt_lp_tokens}"
        );
    }
}
