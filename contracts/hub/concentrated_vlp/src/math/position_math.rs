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

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Uint128, Uint256};

    use crate::math::position_math::{accumulate_fee_growth, fee_growth_inside, fees_owed};
    use crate::state::TickInfo;

    #[test]
    fn fee_growth_inside_calculation_vectors() {
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
            0,
            -10,
            10,
            Uint256::from(100u128),
            Uint256::from(200u128),
            Some(lower),
            Some(upper),
        )
        .unwrap();
        assert_eq!(inside_0, Uint256::from(60u128));
        assert_eq!(inside_1, Uint256::from(140u128));
        assert_eq!(
            fees_owed(Uint128::new(10_000), inside_0, Uint256::from(0u128)).unwrap(),
            Uint128::zero()
        );
    }

    /// fee_growth_inside reverts when global accumulator has wrapped past a
    /// tick's fee_growth_outside value.
    ///
    /// Scenario: A pool with minimum liquidity (1) accumulates enough fees to
    /// push fee_growth_global past Uint256::MAX and wrap around to a small
    /// value. The lower tick was initialized when fee_growth_global was large
    /// (pre-wrap), so its fee_growth_outside is larger than the current global.
    #[test]
    fn fee_growth_inside_reverts_on_wrapped_global() {
        // Pre-wrap: global was near MAX, tick was initialized with outside = MAX - 100
        // Post-wrap: global wrapped to 50
        // True fee growth on the "inside" of the lower tick = 50 - 0 (below) = 50,
        // but computing "below" requires global(50) - outside(MAX-100) which underflows.
        let fee_growth_global = Uint256::from(50u128);
        let fee_growth_outside_large = Uint256::MAX - Uint256::from(100u128);

        let lower = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: 1,
            fee_growth_outside_0_x128: fee_growth_outside_large,
            fee_growth_outside_1_x128: Uint256::zero(),
        };
        let upper = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: -1,
            fee_growth_outside_0_x128: Uint256::zero(),
            fee_growth_outside_1_x128: Uint256::zero(),
        };

        // Current tick is BELOW lower_tick, so the code path hits:
        //   fee_growth_below_0 = global.checked_sub(lower.outside) → REVERTS
        // With wrapping_sub it would correctly return 151 (50 - (MAX-100) mod 2^256 = 151)
        let result = fee_growth_inside(
            -20, // current_tick below lower
            -10, // lower_tick
            10,  // upper_tick
            fee_growth_global,
            Uint256::zero(),
            Some(lower),
            Some(upper),
        );

        // With wrapping_sub: fee_growth_below_0 = 50 - (MAX-100) mod 2^256 = 151
        // fee_growth_above_0 = upper.outside = 0 (current_tick < upper_tick)
        // inside_0 = global(50) - below(151) - above(0) — this also wraps
        // = 50 - 151 mod 2^256 = MAX - 100
        // But we mainly need this to NOT revert.
        let (inside_0, _inside_1) = result.expect("should not revert with wrapping subtraction");

        // Verify the wrapping math produces a coherent value (not zero — fees were accrued)
        assert!(
            !inside_0.is_zero(),
            "fee_growth_inside should be non-zero after wrap"
        );
    }

    /// fees_owed early-return guard masks wrapped delta.
    ///
    /// Even if fee_growth_inside uses wrapping_sub correctly, fees_owed has a
    /// guard: `if fee_growth_inside <= fee_growth_inside_last { return 0 }`.
    /// After a wrap, the current inside value (e.g. 5) can be numerically less
    /// than the last snapshot (e.g. MAX-10), yet 16 fees were truly accrued.
    /// The guard incorrectly returns zero.
    #[test]
    fn fees_owed_guard_masks_wrapped_fee_growth() {
        let liquidity = Uint128::new(1_000_000);
        // Position's last snapshot was near MAX
        let fee_growth_inside_last = Uint256::MAX - Uint256::from(10u128);
        // Current fee_growth_inside wrapped to a value that gives a meaningful Q128 result
        // True delta (wrapping) = fee_growth_inside_now - fee_growth_inside_last mod 2^256
        // = (MAX - 10 + 1) + fee_growth_inside_now = 11 + large_offset
        // We need delta * liquidity / 2^128 > 0, so delta must be >= 2^128 / liquidity
        // 2^128 / 1_000_000 ≈ 3.4e32, so set a delta well above that.
        // Use fee_growth_inside_now such that wrapping delta = 2^128 (yields fees = liquidity = 1_000_000)
        let fee_growth_inside_now = fee_growth_inside_last.wrapping_add(Uint256::one() << 128u32);

        let result = fees_owed(liquidity, fee_growth_inside_now, fee_growth_inside_last).unwrap();

        // True delta (wrapping): 5 - (MAX-10) mod 2^256 = 16
        // fees = liquidity(1_000_000) * 16 / 2^128 — rounds to 0 due to Q128 scaling.
        // Use larger values to get a non-zero result:
        // We'll check this separately below with scaled values.
        // For now, just verify it doesn't return zero from the guard.
        assert!(
            result > Uint128::zero(),
            "fees_owed should return non-zero fees after fee_growth wraps"
        );
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
}
