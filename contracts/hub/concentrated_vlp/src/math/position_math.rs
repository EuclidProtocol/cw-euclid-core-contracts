use cosmwasm_std::{Uint128, Uint256};
use euclid::error::ContractError;

use crate::{
    math::{full_math::mul_div, sqrt_price_math::q128},
    state::TickInfo,
};

/// Computes the fee growth accrued inside a tick range [lower_tick, upper_tick).
///
/// All subtractions use `wrapping_sub` because fee growth accumulators are
/// monotonically increasing Uint256 values that are *designed to overflow*
/// (mod 2^256). The absolute values are
/// meaningless — only the *difference* between two snapshots matters, and
/// wrapping subtraction always produces the correct delta regardless of
/// whether the accumulator has wrapped around.
///
/// Using `checked_sub` here would cause transactions to revert once the
/// global accumulator wraps past any tick's `fee_growth_outside` value,
/// permanently bricking the pool.
pub fn fee_growth_inside(
    current_tick: i64,
    lower_tick: i64,
    upper_tick: i64,
    fee_growth_global_0_x128: Uint256,
    fee_growth_global_1_x128: Uint256,
    lower: Option<TickInfo>,
    upper: Option<TickInfo>,
) -> Result<(Uint256, Uint256), ContractError> {
    let lower = lower.unwrap_or_default();
    let upper = upper.unwrap_or_default();

    // Step 1: fee_growth_below = fees accrued below the lower tick.
    //   If current_tick >= lower_tick, the lower tick's "outside" IS the below side.
    //   Otherwise: fee_growth_below = fee_growth_global - fee_growth_outside (mod 2^256)
    let fee_growth_below_0 = if current_tick >= lower_tick {
        lower.fee_growth_outside_0_x128
    } else {
        fee_growth_global_0_x128.wrapping_sub(lower.fee_growth_outside_0_x128)
    };
    let fee_growth_below_1 = if current_tick >= lower_tick {
        lower.fee_growth_outside_1_x128
    } else {
        fee_growth_global_1_x128.wrapping_sub(lower.fee_growth_outside_1_x128)
    };

    // Step 2: fee_growth_above = fees accrued above the upper tick.
    //   If current_tick < upper_tick, the upper tick's "outside" IS the above side.
    //   Otherwise: fee_growth_above = fee_growth_global - fee_growth_outside (mod 2^256)
    let fee_growth_above_0 = if current_tick < upper_tick {
        upper.fee_growth_outside_0_x128
    } else {
        fee_growth_global_0_x128.wrapping_sub(upper.fee_growth_outside_0_x128)
    };
    let fee_growth_above_1 = if current_tick < upper_tick {
        upper.fee_growth_outside_1_x128
    } else {
        fee_growth_global_1_x128.wrapping_sub(upper.fee_growth_outside_1_x128)
    };

    // Step 3: fee_growth_inside = fee_growth_global - fee_growth_below - fee_growth_above (mod 2^256)
    let inside_0 = fee_growth_global_0_x128
        .wrapping_sub(fee_growth_below_0)
        .wrapping_sub(fee_growth_above_0);
    let inside_1 = fee_growth_global_1_x128
        .wrapping_sub(fee_growth_below_1)
        .wrapping_sub(fee_growth_above_1);

    Ok((inside_0, inside_1))
}

/// Computes the fees owed to a position since its last snapshot.
///
/// Uses `wrapping_sub` for the same reason as `fee_growth_inside`: fee growth
/// values are mod-2^256 accumulators where only deltas matter. After a wrap,
/// the current value may be numerically smaller than the last snapshot, but
/// the wrapping difference is still the true accrued amount.
pub fn fees_owed(
    liquidity: Uint128,
    fee_growth_inside_x128: Uint256,
    fee_growth_inside_last_x128: Uint256,
) -> Result<Uint128, ContractError> {
    if liquidity.is_zero() {
        return Ok(Uint128::zero());
    }
    // Step 1: delta = fee_growth_inside_now - fee_growth_inside_last (mod 2^256)
    let delta = fee_growth_inside_x128.wrapping_sub(fee_growth_inside_last_x128);
    // Step 2: fees = liquidity * delta / 2^128
    //   delta is in Q128 (per unit of liquidity), so dividing by 2^128 gives the raw token amount.
    let amount = mul_div(Uint256::from(liquidity.u128()), delta, q128())?;
    Uint128::try_from(amount).map_err(|_| ContractError::new("fees owed overflow"))
}

/// Accumulates fee growth into the global accumulator.
///
/// Computes: fee_growth_global += lp_fee * 2^128 / active_liquidity (mod 2^256)
///
/// The multiplication by 2^128 converts the raw fee amount into the Q128
/// per-unit-of-liquidity representation. The addition wraps because the
/// global accumulator is designed to overflow — only deltas matter.
pub fn accumulate_fee_growth(
    fee_growth_global_x128: Uint256,
    lp_fee: Uint256,
    active_liquidity: Uint128,
) -> Result<Uint256, ContractError> {
    if lp_fee.is_zero() || active_liquidity.is_zero() {
        return Ok(fee_growth_global_x128);
    }
    // Step 1: fee_growth_delta = lp_fee * 2^128 / active_liquidity
    let fee_growth_delta = lp_fee
        .checked_mul(q128())?
        .checked_div(Uint256::from(active_liquidity.u128()))?;
    // Step 2: fee_growth_global += fee_growth_delta (mod 2^256)
    Ok(fee_growth_global_x128.wrapping_add(fee_growth_delta))
}

/// Flips a tick's fee_growth_outside when the tick is crossed during a swap.
///
/// Formula: new_outside = fee_growth_global - old_outside (mod 2^256)
///
/// This "flips" which side of the tick the outside accumulator refers to.
/// Before crossing, outside tracks fees on one side; after crossing, the
/// current price is on the opposite side, so we subtract to get the complement.
/// Uses wrapping_sub because the global accumulator may have wrapped.
pub fn flip_fee_growth_outside(
    fee_growth_global_x128: Uint256,
    fee_growth_outside_x128: Uint256,
) -> Uint256 {
    fee_growth_global_x128.wrapping_sub(fee_growth_outside_x128)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Uint128, Uint256};

    use crate::math::position_math::{
        accumulate_fee_growth, fee_growth_inside, fees_owed, flip_fee_growth_outside,
    };
    use crate::state::TickInfo;

    #[test]
    fn fee_growth_inside_normal_in_range() {
        // current_tick(0) is between lower(-10) and upper(10)
        // below_0 = lower.outside = 10, above_0 = upper.outside = 30
        // inside_0 = 100 - 10 - 30 = 60
        // inside_1 = 200 - 20 - 40 = 140
        let lower = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: 1,
            fee_growth_outside_0_x128: Uint256::from(10u128),
            fee_growth_outside_1_x128: Uint256::from(20u128),
        };
        let upper = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: -1,
            fee_growth_outside_0_x128: Uint256::from(30u128),
            fee_growth_outside_1_x128: Uint256::from(40u128),
        };
        let (inside_0, inside_1) = fee_growth_inside(
            0, -10, 10,
            Uint256::from(100u128), Uint256::from(200u128),
            Some(lower), Some(upper),
        ).unwrap();
        assert_eq!(inside_0, Uint256::from(60u128));
        assert_eq!(inside_1, Uint256::from(140u128));
    }

    #[test]
    fn fee_growth_inside_none_ticks_use_default() {
        // None ticks default to all-zero TickInfo
        // below = 0 (current >= lower), above = 0 (current < upper)
        // inside = global - 0 - 0 = global
        let (inside_0, inside_1) = fee_growth_inside(
            0, -10, 10,
            Uint256::from(500u128), Uint256::from(700u128),
            None, None,
        ).unwrap();
        assert_eq!(inside_0, Uint256::from(500u128));
        assert_eq!(inside_1, Uint256::from(700u128));
    }

    /// Wrapping when current_tick < lower_tick (fee_growth_below branch).
    /// global(50) - lower.outside(MAX-100) wraps to 151.
    /// inside = global(50) - below(151) - above(0) wraps to MAX-100.
    #[test]
    fn fee_growth_inside_wrapping_below_lower_tick() {
        let global_0 = Uint256::from(50u128);
        let global_1 = Uint256::from(75u128);
        let outside_large_0 = Uint256::MAX - Uint256::from(100u128);
        let outside_large_1 = Uint256::MAX - Uint256::from(200u128);

        let lower = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: 1,
            fee_growth_outside_0_x128: outside_large_0,
            fee_growth_outside_1_x128: outside_large_1,
        };
        let upper = TickInfo::default();

        let (inside_0, inside_1) = fee_growth_inside(
            -20, -10, 10, // current below lower
            global_0, global_1,
            Some(lower), Some(upper),
        ).expect("wrapping_sub should not revert");

        // below_0 = 50 - (MAX-100) mod 2^256 = 151
        // above_0 = upper.outside = 0 (current < upper)
        // inside_0 = 50 - 151 - 0 mod 2^256 = MAX - 100
        assert_eq!(inside_0, Uint256::MAX - Uint256::from(100u128));
        // below_1 = 75 - (MAX-200) mod 2^256 = 276
        // inside_1 = 75 - 276 - 0 mod 2^256 = MAX - 200
        assert_eq!(inside_1, Uint256::MAX - Uint256::from(200u128));
    }

    /// Wrapping when current_tick >= upper_tick (fee_growth_above branch).
    /// global(50) - upper.outside(MAX-100) wraps to 151.
    #[test]
    fn fee_growth_inside_wrapping_above_upper_tick() {
        let global_0 = Uint256::from(50u128);
        let global_1 = Uint256::from(75u128);
        let outside_large_0 = Uint256::MAX - Uint256::from(100u128);
        let outside_large_1 = Uint256::MAX - Uint256::from(200u128);

        let lower = TickInfo::default();
        let upper = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: -1,
            fee_growth_outside_0_x128: outside_large_0,
            fee_growth_outside_1_x128: outside_large_1,
        };

        let (inside_0, inside_1) = fee_growth_inside(
            20, -10, 10, // current above upper
            global_0, global_1,
            Some(lower), Some(upper),
        ).expect("wrapping_sub should not revert");

        // below_0 = lower.outside = 0 (current >= lower)
        // above_0 = 50 - (MAX-100) mod 2^256 = 151
        // inside_0 = 50 - 0 - 151 mod 2^256 = MAX - 100
        assert_eq!(inside_0, Uint256::MAX - Uint256::from(100u128));
        // inside_1 = 75 - 0 - 276 mod 2^256 = MAX - 200
        assert_eq!(inside_1, Uint256::MAX - Uint256::from(200u128));
    }

    /// fees_owed correctly computes fees when fee_growth_inside has wrapped
    /// past fee_growth_inside_last.
    ///
    /// Wrapping delta = 2^128, so fees = liquidity * 2^128 / 2^128 = liquidity.
    #[test]
    fn fees_owed_wrapping_delta() {
        let liquidity = Uint128::new(1_000_000);
        let fee_growth_inside_last = Uint256::MAX - Uint256::from(10u128);
        // Wrapping delta = 2^128
        let fee_growth_inside_now = fee_growth_inside_last.wrapping_add(Uint256::one() << 128u32);

        let result = fees_owed(liquidity, fee_growth_inside_now, fee_growth_inside_last).unwrap();

        // fees = 1_000_000 * 2^128 / 2^128 = 1_000_000
        assert_eq!(result, Uint128::new(1_000_000));
    }

    /// fees_owed returns zero for zero liquidity even with a wrapping delta.
    #[test]
    fn fees_owed_zero_liquidity_with_wrapping_delta() {
        let fee_growth_inside_last = Uint256::MAX - Uint256::from(10u128);
        let fee_growth_inside_now = fee_growth_inside_last.wrapping_add(Uint256::one() << 128u32);

        let result = fees_owed(Uint128::zero(), fee_growth_inside_now, fee_growth_inside_last).unwrap();
        assert_eq!(result, Uint128::zero());
    }

    /// End-to-end: fee_growth_inside with wrapped globals → fees_owed → non-zero fees.
    ///
    /// Simulates a position that was opened before a fee growth wrap and is
    /// now collecting fees after the wrap.
    #[test]
    fn fee_growth_inside_to_fees_owed_end_to_end_wrap() {
        let q128 = Uint256::one() << 128u32;

        // Pool state: global has wrapped to a small value
        let global_0 = Uint256::from(500u128) * q128;
        let global_1 = Uint256::from(300u128) * q128;

        // Ticks with small outside values (initialized after the wrap)
        let lower = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1000),
            liquidity_net: 1000,
            fee_growth_outside_0_x128: Uint256::from(100u128) * q128,
            fee_growth_outside_1_x128: Uint256::from(50u128) * q128,
        };
        let upper = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1000),
            liquidity_net: -1000,
            fee_growth_outside_0_x128: Uint256::from(50u128) * q128,
            fee_growth_outside_1_x128: Uint256::from(30u128) * q128,
        };

        // current_tick in range → below = lower.outside, above = upper.outside
        // inside_0 = 500*q128 - 100*q128 - 50*q128 = 350*q128
        // inside_1 = 300*q128 - 50*q128 - 30*q128 = 220*q128
        let (inside_0, inside_1) = fee_growth_inside(
            0, -10, 10,
            global_0, global_1,
            Some(lower), Some(upper),
        ).unwrap();

        assert_eq!(inside_0, Uint256::from(350u128) * q128);
        assert_eq!(inside_1, Uint256::from(220u128) * q128);

        // Position's last snapshot was before the wrap — near MAX.
        // Wrapping delta for token0: inside_0 - last_0 mod 2^256
        // Set last so that wrapping delta = 200 * q128
        let last_0 = inside_0.wrapping_sub(Uint256::from(200u128) * q128);
        let last_1 = inside_1.wrapping_sub(Uint256::from(100u128) * q128);

        let liquidity = Uint128::new(5_000);

        // fees_0 = 5000 * 200*q128 / q128 = 5000 * 200 = 1_000_000
        let fees_0 = fees_owed(liquidity, inside_0, last_0).unwrap();
        assert_eq!(fees_0, Uint128::new(1_000_000));

        // fees_1 = 5000 * 100*q128 / q128 = 5000 * 100 = 500_000
        let fees_1 = fees_owed(liquidity, inside_1, last_1).unwrap();
        assert_eq!(fees_1, Uint128::new(500_000));
    }

    /// Table-driven tests for accumulate_fee_growth covering normal accumulation,
    /// zero-value short-circuits, and wrapping overflow.
    #[test]
    fn accumulate_fee_growth_table() {
        let q128 = Uint256::one() << 128u32;

        struct Case {
            name: &'static str,
            global: Uint256,
            lp_fee: Uint256,
            liquidity: Uint128,
            expected: Uint256,
        }

        let cases = vec![
            Case {
                name: "zero fee — no change",
                global: Uint256::from(1000u128),
                lp_fee: Uint256::zero(),
                liquidity: Uint128::new(500),
                expected: Uint256::from(1000u128),
            },
            Case {
                name: "zero liquidity — no change",
                global: Uint256::from(1000u128),
                lp_fee: Uint256::from(50u128),
                liquidity: Uint128::zero(),
                expected: Uint256::from(1000u128),
            },
            Case {
                name: "normal accumulation — fee=100, liquidity=1",
                // delta = 100 * 2^128 / 1 = 100 * 2^128
                global: Uint256::zero(),
                lp_fee: Uint256::from(100u128),
                liquidity: Uint128::new(1),
                expected: Uint256::from(100u128) * q128,
            },
            Case {
                name: "normal accumulation — fee=100, liquidity=50",
                // delta = 100 * 2^128 / 50 = 2 * 2^128
                global: Uint256::zero(),
                lp_fee: Uint256::from(100u128),
                liquidity: Uint128::new(50),
                expected: Uint256::from(2u128) * q128,
            },
            Case {
                name: "rounds down — fee=10, liquidity=3",
                // delta = 10 * 2^128 / 3 = 3 * 2^128 + remainder (truncated)
                // 10 * 2^128 = 3402823669209384634633746074317682114560
                // / 3 = 1134274556403128211544582024772560704853 (truncated)
                global: Uint256::zero(),
                lp_fee: Uint256::from(10u128),
                liquidity: Uint128::new(3),
                expected: Uint256::from(10u128) * q128 / Uint256::from(3u128),
            },
            Case {
                name: "accumulates on top of existing global",
                // delta = 10 * 2^128 / 10 = 2^128
                global: Uint256::from(500u128),
                lp_fee: Uint256::from(10u128),
                liquidity: Uint128::new(10),
                expected: Uint256::from(500u128) + q128,
            },
            Case {
                name: "wrapping overflow — global near MAX",
                // delta = 1 * 2^128 / 1 = 2^128
                // (MAX - 100) + 2^128 wraps to 2^128 - 101
                global: Uint256::MAX - Uint256::from(100u128),
                lp_fee: Uint256::one(),
                liquidity: Uint128::new(1),
                expected: q128 - Uint256::from(101u128),
            },
            Case {
                name: "wrapping overflow — global is MAX",
                // delta = 1 * 2^128 / 1 = 2^128
                // MAX + 2^128 wraps to 2^128 - 1
                global: Uint256::MAX,
                lp_fee: Uint256::one(),
                liquidity: Uint128::new(1),
                expected: q128 - Uint256::one(),
            },
        ];

        for case in cases {
            let result = accumulate_fee_growth(case.global, case.lp_fee, case.liquidity)
                .unwrap_or_else(|e| panic!("{}: unexpected error: {}", case.name, e));
            assert_eq!(result, case.expected, "FAILED: {}", case.name);
        }
    }

    /// H-2: Tick crossing flips fee_growth_outside via global - outside (mod 2^256).
    /// Must not revert when global has wrapped past outside.
    #[test]
    fn flip_fee_growth_outside_table() {
        struct Case {
            name: &'static str,
            global: Uint256,
            outside: Uint256,
            expected: Uint256,
        }

        let cases = vec![
            Case {
                name: "normal — global > outside",
                global: Uint256::from(1000u128),
                outside: Uint256::from(300u128),
                expected: Uint256::from(700u128),
            },
            Case {
                name: "equal — no fees on either side",
                global: Uint256::from(500u128),
                outside: Uint256::from(500u128),
                expected: Uint256::zero(),
            },
            Case {
                name: "outside is zero",
                global: Uint256::from(1000u128),
                outside: Uint256::zero(),
                expected: Uint256::from(1000u128),
            },
            Case {
                name: "global is zero, outside is zero",
                global: Uint256::zero(),
                outside: Uint256::zero(),
                expected: Uint256::zero(),
            },
            Case {
                name: "wrapped — global < outside after overflow",
                // global wrapped to 50, outside was set at MAX - 100 before wrap
                // 50 - (MAX - 100) mod 2^256 = 151
                global: Uint256::from(50u128),
                outside: Uint256::MAX - Uint256::from(100u128),
                expected: Uint256::from(151u128),
            },
            Case {
                name: "double flip is identity",
                global: Uint256::from(50u128),
                outside: Uint256::MAX - Uint256::from(100u128),
                // flip once: 151, flip again: 50 - 151 mod 2^256 = MAX - 100
                expected: Uint256::from(151u128),
            },
        ];

        for case in cases {
            let result = flip_fee_growth_outside(case.global, case.outside);
            assert_eq!(result, case.expected, "FAILED: {}", case.name);
        }

        // Verify double-flip invariant: flip(flip(outside)) == outside
        let global = Uint256::from(50u128);
        let outside = Uint256::MAX - Uint256::from(100u128);
        let flipped = flip_fee_growth_outside(global, outside);
        let double_flipped = flip_fee_growth_outside(global, flipped);
        assert_eq!(double_flipped, outside, "double flip should restore original");
    }
}
