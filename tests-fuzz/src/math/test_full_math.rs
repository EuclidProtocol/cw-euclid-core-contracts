use concentrated_vlp::math::full_math::{div_rounding_up, mul_div, mul_div_rounding_up};
use cosmwasm_std::Uint256;
use proptest::prelude::*;

fn arb_uint256_small() -> impl Strategy<Value = Uint256> {
    (0u128..=u64::MAX as u128).prop_map(Uint256::from)
}

fn arb_uint256_nonzero() -> impl Strategy<Value = Uint256> {
    (1u128..=u64::MAX as u128).prop_map(Uint256::from)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn rounding_consistency(
        a in arb_uint256_small(),
        b in arb_uint256_small(),
        d in arb_uint256_nonzero(),
    ) {
        if let (Ok(floor), Ok(ceil)) = (mul_div(a, b, d), mul_div_rounding_up(a, b, d)) {
            let diff = ceil - floor;
            prop_assert!(diff <= Uint256::one(), "ceil - floor must be 0 or 1, got {}", diff);
        }
    }

    #[test]
    fn identity(a in arb_uint256_small(), d in arb_uint256_nonzero()) {
        let result = mul_div(a, d, d);
        prop_assert!(result.is_ok());
        prop_assert_eq!(result.unwrap(), a);
    }

    #[test]
    fn div_rounding_up_covers(
        n in arb_uint256_small(),
        d in arb_uint256_nonzero(),
    ) {
        if let Ok(result) = div_rounding_up(n, d) {
            // result * d >= n (ceil property)
            if let Ok(product) = result.checked_mul(d) {
                prop_assert!(product >= n, "div_rounding_up * d should >= n");
            }
        }
    }

    #[test]
    fn zero_numerator(d in arb_uint256_nonzero()) {
        let result = mul_div(Uint256::zero(), Uint256::from(42u128), d).unwrap();
        prop_assert_eq!(result, Uint256::zero());
    }

    #[test]
    fn division_by_zero_fails(a in arb_uint256_small(), b in arb_uint256_small()) {
        prop_assert!(mul_div(a, b, Uint256::zero()).is_err());
    }
}
