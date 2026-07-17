use concentrated_vlp::math::tick_math::{
    get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, max_sqrt_ratio, min_sqrt_ratio,
};
use concentrated_vlp::state::{MAX_TICK, MIN_TICK};
use cosmwasm_std::Uint256;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10000))]

    #[test]
    fn tick_roundtrip(tick in MIN_TICK..=MAX_TICK) {
        let sqrt_ratio = get_sqrt_ratio_at_tick(tick).unwrap();
        let recovered = get_tick_at_sqrt_ratio(sqrt_ratio).unwrap();
        prop_assert_eq!(recovered, tick);
    }

    #[test]
    fn tick_monotonicity(tick in MIN_TICK..MAX_TICK) {
        let sqrt_a = get_sqrt_ratio_at_tick(tick).unwrap();
        let sqrt_b = get_sqrt_ratio_at_tick(tick + 1).unwrap();
        prop_assert!(sqrt_a < sqrt_b, "sqrt ratio must be strictly increasing with tick");
    }

    #[test]
    fn tick_bounds(tick in MIN_TICK..=MAX_TICK) {
        let sqrt_ratio = get_sqrt_ratio_at_tick(tick).unwrap();
        prop_assert!(sqrt_ratio >= min_sqrt_ratio());
        prop_assert!(sqrt_ratio <= max_sqrt_ratio());
    }

    #[test]
    fn invalid_tick_low(tick in i64::MIN..MIN_TICK) {
        prop_assert!(get_sqrt_ratio_at_tick(tick).is_err());
    }

    #[test]
    fn invalid_tick_high(tick in (MAX_TICK + 1)..i64::MAX) {
        prop_assert!(get_sqrt_ratio_at_tick(tick).is_err());
    }

    #[test]
    fn inverse_consistency(tick in MIN_TICK..MAX_TICK) {
        let sqrt_ratio = get_sqrt_ratio_at_tick(tick).unwrap();
        let sqrt_ratio_next = get_sqrt_ratio_at_tick(tick + 1).unwrap();
        let recovered = get_tick_at_sqrt_ratio(sqrt_ratio).unwrap();
        prop_assert_eq!(recovered, tick);
        // Any ratio strictly between tick and tick+1 should resolve to tick
        if sqrt_ratio_next > sqrt_ratio + Uint256::one() {
            let mid = sqrt_ratio + Uint256::one();
            let mid_tick = get_tick_at_sqrt_ratio(mid).unwrap();
            prop_assert_eq!(mid_tick, tick);
        }
    }
}
