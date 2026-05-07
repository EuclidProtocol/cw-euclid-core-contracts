use crate::{error::ContractError, voucher::VOUCHER_DECIMAL};
use cosmwasm_std::Uint256;

pub fn normalize_token_to_voucher(
    token_amount: Uint256,
    decimals: u32,
) -> Result<Uint256, ContractError> {
    if decimals > VOUCHER_DECIMAL {
        return Err(ContractError::new(&format!(
            "Token decimals {} exceeds voucher precision {}",
            decimals, VOUCHER_DECIMAL
        )));
    }
    normalize(token_amount, decimals, VOUCHER_DECIMAL)
}

pub fn normalize_voucher_to_token(
    voucher_amount: Uint256,
    decimals: u32,
) -> Result<Uint256, ContractError> {
    if decimals > VOUCHER_DECIMAL {
        return Err(ContractError::new(&format!(
            "Token decimals {} exceeds voucher precision {}",
            decimals, VOUCHER_DECIMAL
        )));
    }
    normalize(voucher_amount, VOUCHER_DECIMAL, decimals)
}

pub fn normalize(
    amount: Uint256,
    from_decimals: u32,
    to_decimals: u32,
) -> Result<Uint256, ContractError> {
    if from_decimals == to_decimals {
        return Ok(amount);
    }
    if to_decimals > from_decimals {
        let factor = Uint256::from(10u128).checked_pow(to_decimals - from_decimals)?;
        Ok(amount.checked_mul(factor)?)
    } else {
        let factor = Uint256::from(10u128).checked_pow(from_decimals - to_decimals)?;
        Ok(amount.checked_div(factor)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::one_usdc_6dec(1_000_000u128, 6, 1_000_000_000_000_000_000_000_000u128)]
    #[case::one_micro_6dec(1u128, 6, 1_000_000_000_000_000_000u128)]
    #[case::zero_6dec(0u128, 6, 0u128)]
    #[case::fractional_usdc_6dec(999_999u128, 6, 999_999_000_000_000_000_000_000u128)]
    #[case::one_satoshi_8dec(1u128, 8, 10_000_000_000_000_000u128)]
    #[case::one_btc_8dec(100_000_000u128, 8, 1_000_000_000_000_000_000_000_000u128)]
    #[case::one_wei_18dec(1u128, 18, 1_000_000u128)]
    #[case::one_eth_18dec(
        1_000_000_000_000_000_000u128,
        18,
        1_000_000_000_000_000_000_000_000u128
    )]
    #[case::identity_24dec(1u128, 24, 1u128)]
    fn test_normalize_token_to_voucher(
        #[case] raw: u128,
        #[case] decimals: u32,
        #[case] expected: u128,
    ) {
        let result = normalize_token_to_voucher(Uint256::from(raw), decimals)
            .expect("normalize to voucher failed");
        assert_eq!(result, Uint256::from(expected));
    }

    #[rstest]
    #[case::one_usdc_6dec(1_000_000_000_000_000_000_000_000u128, 6, 1_000_000u128)]
    #[case::one_micro_6dec(1_000_000_000_000_000_000u128, 6, 1u128)]
    #[case::zero_6dec(0u128, 6, 0u128)]
    #[case::truncates_below_one_micro(500_000_000_000_000_000u128, 6, 0u128)]
    #[case::truncates_fractional(1_999_999_999_999_999_999u128, 6, 1u128)]
    #[case::one_eth_18dec(
        1_000_000_000_000_000_000_000_000u128,
        18,
        1_000_000_000_000_000_000u128
    )]
    #[case::one_wei_18dec(1_000_000u128, 18, 1u128)]
    #[case::truncates_below_one_wei(999_999u128, 18, 0u128)]
    #[case::identity_24dec(1u128, 24, 1u128)]
    fn test_normalize_voucher_to_token(
        #[case] voucher: u128,
        #[case] decimals: u32,
        #[case] expected: u128,
    ) {
        let result = normalize_voucher_to_token(Uint256::from(voucher), decimals)
            .expect("normalize voucher to token failed");
        assert_eq!(result, Uint256::from(expected));
    }

    #[rstest]
    #[case::dec_6(6)]
    #[case::dec_8(8)]
    #[case::dec_12(12)]
    #[case::dec_18(18)]
    #[case::dec_24(24)]
    fn test_roundtrip_lossless_for_whole_units(#[case] decimals: u32) {
        let raw = Uint256::from(1_000_000u128);
        let voucher =
            normalize_token_to_voucher(raw, decimals).expect("normalize to voucher failed");
        let back =
            normalize_voucher_to_token(voucher, decimals).expect("normalize back to token failed");
        assert_eq!(back, raw);
    }

    #[rstest]
    #[case::six_to_six(100u128, 6, 6)]
    #[case::eighteen_to_eighteen(42u128, 18, 18)]
    #[case::twentyfour_to_twentyfour(1u128, 24, 24)]
    fn test_normalize_same_decimals_identity(
        #[case] amount: u128,
        #[case] from: u32,
        #[case] to: u32,
    ) {
        let result = normalize(Uint256::from(amount), from, to).expect("normalize identity failed");
        assert_eq!(result, Uint256::from(amount));
    }

    #[test]
    fn test_normalize_zero_amount() {
        assert_eq!(
            normalize(Uint256::zero(), 6, 24).expect("normalize zero upscale failed"),
            Uint256::zero()
        );
        assert_eq!(
            normalize(Uint256::zero(), 24, 6).expect("normalize zero downscale failed"),
            Uint256::zero()
        );
    }

    #[test]
    fn test_normalize_token_to_voucher_overflow() {
        let huge = Uint256::MAX;
        let err = normalize_token_to_voucher(huge, 6).expect_err("should overflow");
        assert!(
            matches!(err, ContractError::Overflow(_)),
            "expected overflow error, got {:?}",
            err
        );
    }

    #[rstest]
    #[case::smallest_6dec(1u128, 6)]
    #[case::arbitrary_8dec(123_456_789u128, 8)]
    #[case::one_eth_18dec(1_000_000_000_000_000_000u128, 18)]
    fn test_roundtrip_preserves_value(#[case] raw: u128, #[case] decimals: u32) {
        let voucher = normalize_token_to_voucher(Uint256::from(raw), decimals)
            .expect("normalize to voucher failed");
        let back =
            normalize_voucher_to_token(voucher, decimals).expect("normalize back to token failed");
        assert_eq!(back, Uint256::from(raw));
    }

    #[rstest]
    #[case::scale_up_6_to_18(6, 18)]
    #[case::scale_down_18_to_6(18, 6)]
    #[case::scale_up_8_to_12(8, 12)]
    #[case::scale_down_12_to_8(12, 8)]
    fn test_normalize_between_arbitrary_decimals(#[case] from: u32, #[case] to: u32) {
        let amount = Uint256::from(1_000_000u128);
        let normalized = normalize(amount, from, to).expect("normalize forward failed");
        let back = normalize(normalized, to, from).expect("normalize reverse failed");
        if to > from {
            assert_eq!(back, amount);
        } else {
            let factor = Uint256::from(10u128).pow(from - to);
            let expected_truncation = (amount / factor) * factor;
            assert_eq!(back, expected_truncation);
            assert!(back <= amount, "back must be less than or equal to amount");
        }
    }

    #[rstest]
    #[case::dec_25(25)]
    #[case::dec_30(30)]
    #[case::dec_36(36)]
    fn test_normalize_token_to_voucher_rejects_decimals_above_24(#[case] decimals: u32) {
        let err = normalize_token_to_voucher(Uint256::from(1_000_000u128), decimals)
            .expect_err("should reject decimals > 24");
        assert!(
            err.to_string().contains("exceeds voucher precision"),
            "unexpected error: {:?}",
            err
        );
    }

    #[rstest]
    #[case::dec_25(25)]
    #[case::dec_30(30)]
    #[case::dec_36(36)]
    fn test_normalize_voucher_to_token_rejects_decimals_above_24(#[case] decimals: u32) {
        let err = normalize_voucher_to_token(Uint256::from(1u128), decimals)
            .expect_err("should reject decimals > 24");
        assert!(
            err.to_string().contains("exceeds voucher precision"),
            "unexpected error: {:?}",
            err
        );
    }

    #[rstest]
    #[case::scale_down_30_to_24(1_000_000u128, 30, 24, 1u128)]
    #[case::scale_down_36_to_24(1_000_000_000_000u128, 36, 24, 1u128)]
    #[case::scale_down_25_to_24(10u128, 25, 24, 1u128)]
    #[case::scale_down_30_truncates(999_999u128, 30, 24, 0u128)]
    #[case::scale_up_24_to_30(1u128, 24, 30, 1_000_000u128)]
    #[case::scale_up_24_to_36(1u128, 24, 36, 1_000_000_000_000u128)]
    #[case::scale_up_24_to_25(1u128, 24, 25, 10u128)]
    fn test_normalize_base_fn_handles_above_24(
        #[case] amount: u128,
        #[case] from: u32,
        #[case] to: u32,
        #[case] expected: u128,
    ) {
        let result = normalize(Uint256::from(amount), from, to).expect("normalize above 24 failed");
        assert_eq!(result, Uint256::from(expected));
    }

    #[rstest]
    #[case::dec_30(30)]
    #[case::dec_36(36)]
    fn test_normalize_base_fn_roundtrip_above_24(#[case] decimals: u32) {
        let amount = Uint256::from(10u128.pow(decimals));
        let normalized = normalize(amount, decimals, 24).expect("normalize forward failed");
        let back = normalize(normalized, 24, decimals).expect("normalize reverse failed");
        assert_eq!(back, amount);
    }

    // Uint128 → Uint256 → normalize → denormalize → Uint128 roundtrip
    #[rstest]
    #[case::one_usdc_6dec(1_000_000u128, 6)]
    #[case::max_usdc_supply_6dec(100_000_000_000_000u128, 6)] // 100M USDC
    #[case::one_eth_18dec(1_000_000_000_000_000_000u128, 18)]
    #[case::one_btc_8dec(100_000_000u128, 8)]
    #[case::one_unit_6dec(1u128, 6)]
    #[case::one_unit_18dec(1u128, 18)]
    #[case::uint128_max_24dec(u128::MAX, 24)] // identity, no scaling
    #[case::uint128_max_1dec(u128::MAX, 1)] // Maximum scalling possible
    #[case::large_12dec(999_999_999_999_999u128, 12)]
    fn test_uint128_roundtrip_preserves_value(#[case] raw: u128, #[case] decimals: u32) {
        use cosmwasm_std::Uint128;

        let original = Uint128::from(raw);
        let as_256 = Uint256::from(original.u128());
        let voucher =
            normalize_token_to_voucher(as_256, decimals).expect("normalize to voucher failed");
        let back_256 =
            normalize_voucher_to_token(voucher, decimals).expect("normalize back to token failed");
        let back_128 = Uint128::try_from(back_256).expect("Uint256 to Uint128 conversion failed");
        assert_eq!(back_128, original);
    }

    // Uint128 → normalize → math ops → denormalize → Uint128
    // Simulates real flows: deposit, swap partial, withdraw remainder
    #[rstest]
    #[case::add_then_denorm_exact(
        1_000_000u128, 6,       // 1 USDC
        500_000u128, 6,         // + 0.5 USDC
        1_500_000u128           // = 1.5 USDC exact
    )]
    #[case::add_quarter_eth_exact(
        750_000_000_000_000_000u128, 18, // 0.75 ETH
        250_000_000_000_000_000u128, 18, // + 0.25 ETH
        1_000_000_000_000_000_000u128    // = 1 ETH exact
    )]
    #[case::multiply_by_2_exact(
        50_000_000u128, 8,      // 0.5 BTC
        50_000_000u128, 8,      // + 0.5 BTC (simulates doubling)
        100_000_000u128         // = 1 BTC exact
    )]
    fn test_uint128_math_ops_exact(
        #[case] amount_a: u128,
        #[case] decimals_a: u32,
        #[case] amount_b: u128,
        #[case] _decimals_b: u32,
        #[case] expected_raw: u128,
    ) {
        use cosmwasm_std::Uint128;

        let voucher_a = normalize_token_to_voucher(Uint256::from(amount_a), decimals_a)
            .expect("normalize amount_a failed");
        let voucher_b = normalize_token_to_voucher(Uint256::from(amount_b), decimals_a)
            .expect("normalize amount_b failed");
        let result_voucher = voucher_a
            .checked_add(voucher_b)
            .expect("voucher addition overflowed");
        let result_raw = normalize_voucher_to_token(result_voucher, decimals_a)
            .expect("denormalize result failed");
        let result_128 =
            Uint128::try_from(result_raw).expect("Uint256 to Uint128 conversion failed");
        assert_eq!(result_128, Uint128::from(expected_raw));
    }

    // Math ops that produce truncation when denormalizing
    // Expected result computed via Uint128 integer division (same truncation behavior)
    #[rstest]
    #[case::divide_by_3_truncates_6dec(1_000_000u128, 6, 3u128)]
    #[case::divide_by_7_truncates_8dec(100_000_000u128, 8, 7u128)]
    #[case::divide_by_3_truncates_18dec(1_000_000_000_000_000_000u128, 18, 3u128)]
    #[case::divide_large_by_prime_6dec(999_999u128, 6, 13u128)]
    #[case::tiny_division_truncates_to_zero_6dec(1u128, 6, 2u128)]
    #[case::divide_by_1000_8dec(100_000_000u128, 8, 1000u128)]
    #[case::divide_by_prime_18dec(5_000_000_000_000_000_000u128, 18, 17u128)]
    fn test_uint128_math_ops_truncated(
        #[case] raw: u128,
        #[case] decimals: u32,
        #[case] divisor: u128,
    ) {
        use cosmwasm_std::Uint128;

        // Simulate same operation in Uint128 space (integer division truncates identically)
        let expected_128 = Uint128::from(raw) / Uint128::from(divisor);

        // Perform via voucher normalization path
        let voucher =
            normalize_token_to_voucher(Uint256::from(raw), decimals).expect("normalize raw failed");
        let divided = voucher
            .checked_div(Uint256::from(divisor))
            .expect("voucher division failed");
        let result_raw =
            normalize_voucher_to_token(divided, decimals).expect("denormalize result failed");
        let result_128 =
            Uint128::try_from(result_raw).expect("Uint256 to Uint128 conversion failed");

        assert_eq!(result_128, expected_128);
    }

    // Voucher amounts that exceed Uint128::MAX — direct try_from would fail,
    // but denormalizing first brings them back into Uint128 range
    #[rstest]
    #[case::large_6dec(1_000_000_000_000_000_000_000u128, 6)] // 10^21 * 10^18 = 10^39 > u128::MAX
    #[case::large_8dec(100_000_000_000_000_000_000_000u128, 8)] // 10^23 * 10^16 = 10^39
    #[case::large_18dec(1_000_000_000_000_000_000_000_000_000_000_000u128, 18)] // 10^33 * 10^6 = 10^39
    #[case::max_uint128_6dec(u128::MAX, 6)]
    #[case::max_uint128_8dec(u128::MAX, 8)]
    fn test_voucher_exceeds_uint128_without_denormalize(#[case] raw: u128, #[case] decimals: u32) {
        use cosmwasm_std::Uint128;

        let voucher = normalize_token_to_voucher(Uint256::from(raw), decimals)
            .expect("normalize to voucher failed");

        // Direct Uint128 conversion of voucher amount must fail (exceeds u128::MAX)
        assert!(
            Uint128::try_from(voucher).is_err(),
            "voucher {} should exceed Uint128::MAX but didn't",
            voucher
        );

        // Correct path: denormalize first, then convert
        let denormalized =
            normalize_voucher_to_token(voucher, decimals).expect("denormalize failed");
        let result = Uint128::try_from(denormalized)
            .expect("Uint256 to Uint128 conversion failed after denormalize");
        assert_eq!(result, Uint128::from(raw));
    }
}
