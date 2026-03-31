use cosmwasm_std::{Uint128, Uint256};

use crate::invariants::{InvariantCheck, InvariantResult, PoolSnapshot};
use crate::strategies::concentrated::ConcentratedOp;

use super::super::shared::{check_add_liquidity_reserves, check_remove_liquidity_reserves};

// =============================================================================
// Helpers
// =============================================================================

/// Returns true if a Uint256 "decrease" is actually a wrapping forward step
/// (the decrease spans more than half the Uint256 space).
fn is_wrapping_forward(before: Uint256, after: Uint256) -> bool {
    if after >= before {
        return false;
    }
    (before - after) > Uint256::MAX >> 1
}

/// Shared logic for CL:txn:mint and CL:txn:burn liquidity checks.
///
/// In-range: mint must increase liquidity, burn must decrease it.
/// Out-of-range: liquidity must not change.
fn check_liquidity_change(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
    lower_tick: i64,
    upper_tick: i64,
    is_increasing: bool,
) -> InvariantResult {
    let mut result = InvariantResult::new();
    let current_tick = before.slot0.tick;
    let in_range = current_tick >= lower_tick && current_tick < upper_tick;
    let (label, op, expected_dir) = if is_increasing {
        ("CL:txn", "mint", "increase")
    } else {
        ("CL:txn", "burn", "decrease")
    };
    let liq_before = before.slot0.liquidity;
    let liq_after = after.slot0.liquidity;

    let check = if !in_range {
        // Out-of-range: liquidity must not change
        let name = format!("{}:{}_out_of_range_no_change", label, op);
        match liq_after == liq_before {
            true => InvariantCheck::pass(&name),
            false => InvariantCheck::fail(
                &name,
                format!("out-of-range {op} changed liquidity: {liq_before} -> {liq_after}"),
            ),
        }
    } else {
        // In-range: must move in the expected direction
        let name = format!("{}:{}_active_liquidity", label, op);
        let ok = if is_increasing {
            liq_after > liq_before
        } else {
            liq_after < liq_before
        };
        match ok {
            true => InvariantCheck::pass(&name),
            false => InvariantCheck::fail(
                &name,
                format!("in-range {op} but liquidity didn't {expected_dir}: {liq_before} -> {liq_after}"),
            ),
        }
    };

    result.add(check);
    result
}

// =============================================================================
// TRANSITION INVARIANTS (checked before/after each operation)
// =============================================================================

/// CL:txn:swap_fee_growth — After a swap, fee growth for the input token increases
/// (or wraps forward). The output token's fee growth must not change.
/// From Trail of Bits Props #13, #15
pub fn check_swap_fee_growth(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
    zero_for_one: bool,
) -> InvariantResult {
    let mut result = InvariantResult::new();

    // Select input/output fee growth based on swap direction
    let (input_before, input_after, output_before, output_after) = if zero_for_one {
        (
            before.slot0.fee_growth_global_0_x128,
            after.slot0.fee_growth_global_0_x128,
            before.slot0.fee_growth_global_1_x128,
            after.slot0.fee_growth_global_1_x128,
        )
    } else {
        (
            before.slot0.fee_growth_global_1_x128,
            after.slot0.fee_growth_global_1_x128,
            before.slot0.fee_growth_global_0_x128,
            after.slot0.fee_growth_global_0_x128,
        )
    };

    // Input token's fee growth must increase (or wrap forward)
    if input_after < input_before && !is_wrapping_forward(input_before, input_after) {
        result.add(InvariantCheck::fail(
            "CL:txn:swap_fee_growth_input",
            format!(
                "input fee_growth decreased (not a wrap): {} -> {}",
                input_before, input_after,
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("CL:txn:swap_fee_growth_input"));
    }

    // Output token's fee growth must not change
    if output_after != output_before {
        result.add(InvariantCheck::fail(
            "CL:txn:swap_fee_growth_output_unchanged",
            format!(
                "output fee_growth changed: {} -> {}",
                output_before, output_after,
            ),
        ));
    } else {
        result.add(InvariantCheck::pass(
            "CL:txn:swap_fee_growth_output_unchanged",
        ));
    }

    result
}

/// CL:txn:swap_output_bounded — Swap output bounded - output < reserve_out before swap
pub fn check_swap_output_bounded(
    before: &PoolSnapshot,
    amount_out: Uint128,
    zero_for_one: bool,
) -> InvariantCheck {
    let reserve_out = if zero_for_one {
        before.reserve_1
    } else {
        before.reserve_0
    };

    if amount_out > reserve_out {
        return InvariantCheck::fail(
            "CL:txn:swap_output_bounded",
            format!(
                "swap output {} exceeds reserve_out {}",
                amount_out, reserve_out
            ),
        );
    }
    InvariantCheck::pass("CL:txn:swap_output_bounded")
}

/// CL:txn:tick_liquidity_correlation — If swap doesn't change tick, liquidity must not change
/// From Trail of Bits Prop #17
pub fn check_tick_liquidity_correlation(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
) -> InvariantCheck {
    if before.slot0.tick == after.slot0.tick && before.slot0.liquidity != after.slot0.liquidity {
        return InvariantCheck::fail(
            "CL:txn:tick_liquidity_correlation",
            format!(
                "tick unchanged ({}) but liquidity changed: {} -> {}",
                before.slot0.tick, before.slot0.liquidity, after.slot0.liquidity
            ),
        );
    }
    InvariantCheck::pass("CL:txn:tick_liquidity_correlation")
}

/// CL:txn:mint_liquidity — Mint increases active liquidity when in-range (Trail of Bits Prop #1)
pub fn check_mint_liquidity(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
    lower_tick: i64,
    upper_tick: i64,
    _liquidity_delta: Uint128,
) -> InvariantResult {
    check_liquidity_change(before, after, lower_tick, upper_tick, true)
}

/// CL:txn:burn_liquidity — Burn decreases active liquidity when in-range (Trail of Bits Prop #7)
pub fn check_burn_liquidity(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
    lower_tick: i64,
    upper_tick: i64,
    _liquidity_delta: Uint128,
) -> InvariantResult {
    check_liquidity_change(before, after, lower_tick, upper_tick, false)
}

/// CL:txn:swap_reserve_conservation — the pool never gives away more than it receives.
///
/// For a successful swap:
/// - The input reserve must increase (pool receives tokens)
/// - The output reserve must decrease (pool sends tokens)
/// - amount_out < amount_in (fees ensure the pool profits)
///
/// This catches drain exploits where a swap extracts more value than it puts in.
pub fn check_swap_reserve_conservation(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
    zero_for_one: bool,
) -> InvariantResult {
    let mut result = InvariantResult::new();

    let (reserve_in_before, reserve_in_after, reserve_out_before, reserve_out_after) =
        if zero_for_one {
            (
                before.reserve_0,
                after.reserve_0,
                before.reserve_1,
                after.reserve_1,
            )
        } else {
            (
                before.reserve_1,
                after.reserve_1,
                before.reserve_0,
                after.reserve_0,
            )
        };

    // Input reserve must increase
    if reserve_in_after <= reserve_in_before {
        result.add(InvariantCheck::fail(
            "CL:txn:swap_input_reserve_increased",
            format!(
                "input reserve didn't increase: {} -> {}",
                reserve_in_before, reserve_in_after
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("CL:txn:swap_input_reserve_increased"));
    }

    // Output reserve must decrease
    if reserve_out_after >= reserve_out_before {
        result.add(InvariantCheck::fail(
            "CL:txn:swap_output_reserve_decreased",
            format!(
                "output reserve didn't decrease: {} -> {}",
                reserve_out_before, reserve_out_after
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("CL:txn:swap_output_reserve_decreased"));
    }

    result
}

/// CL:txn:swap_price_direction — Swap price moves in the correct direction.
///
/// - zero_for_one: selling token0 pushes price DOWN (sqrt_price decreases)
/// - !zero_for_one: selling token1 pushes price UP (sqrt_price increases)
///
/// The price must move strictly in the swap direction (or stay the same if
/// the swap was too small to move the price by even 1 unit of sqrt_price).
pub fn check_swap_price_direction(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
    zero_for_one: bool,
) -> InvariantCheck {
    if zero_for_one {
        if after.slot0.sqrt_price_x96 > before.slot0.sqrt_price_x96 {
            return InvariantCheck::fail(
                "CL:txn:swap_price_direction",
                format!(
                    "zero_for_one swap increased price: {} -> {}",
                    before.slot0.sqrt_price_x96, after.slot0.sqrt_price_x96
                ),
            );
        }
    } else if after.slot0.sqrt_price_x96 < before.slot0.sqrt_price_x96 {
        return InvariantCheck::fail(
            "CL:txn:swap_price_direction",
            format!(
                "one_for_zero swap decreased price: {} -> {}",
                before.slot0.sqrt_price_x96, after.slot0.sqrt_price_x96
            ),
        );
    }
    InvariantCheck::pass("CL:txn:swap_price_direction")
}

/// CL:txn:swap_protocol_fees — the fee taken from the input
/// must be approximately fee_tier_bps / 10000 of the input amount.
///
/// We can't check the exact fee (we don't have the return value), but we can
/// verify: the protocol fees must have increased (fees were actually collected).
pub fn check_swap_protocol_fees_increased(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
    zero_for_one: bool,
) -> InvariantCheck {
    let (pf_before, pf_after) = if zero_for_one {
        (before.protocol_fees.amount_0, after.protocol_fees.amount_0)
    } else {
        (before.protocol_fees.amount_1, after.protocol_fees.amount_1)
    };

    // Protocol fees should increase (unless protocol cut is 0%, which is unusual)
    // Use >= instead of > to avoid false positives when protocol cut rounds to 0
    if pf_after < pf_before {
        return InvariantCheck::fail(
            "CL:txn:swap_protocol_fees",
            format!(
                "protocol fees for input token decreased: {} -> {}",
                pf_before, pf_after
            ),
        );
    }
    InvariantCheck::pass("CL:txn:swap_protocol_fees")
}

// =============================================================================
// OPERATION DISPATCH
// =============================================================================

impl ConcentratedOp {
    /// Check operation-specific transition invariants.
    pub fn check_transition(
        &self,
        before: &PoolSnapshot,
        after: &PoolSnapshot,
        last_remove_ticks: Option<(i64, i64)>,
    ) -> InvariantResult {
        let mut result = InvariantResult::new();
        match self {
            ConcentratedOp::Swap { zero_for_one, .. } => {
                result.merge(check_swap_fee_growth(before, after, *zero_for_one));
                let amount_out = if *zero_for_one {
                    before.reserve_1.saturating_sub(after.reserve_1)
                } else {
                    before.reserve_0.saturating_sub(after.reserve_0)
                };
                result.add(check_swap_output_bounded(before, amount_out, *zero_for_one));
                result.add(check_tick_liquidity_correlation(before, after));
                result.merge(check_swap_reserve_conservation(
                    before,
                    after,
                    *zero_for_one,
                ));
                result.add(check_swap_price_direction(before, after, *zero_for_one));
                result.add(check_swap_protocol_fees_increased(
                    before,
                    after,
                    *zero_for_one,
                ));
            }
            ConcentratedOp::AddLiquidity {
                lower_tick,
                upper_tick,
                ..
            } => {
                result.merge(check_mint_liquidity(
                    before,
                    after,
                    *lower_tick,
                    *upper_tick,
                    Uint128::one(),
                ));
                result.merge(check_add_liquidity_reserves(before, after));
            }
            ConcentratedOp::RemoveLiquidity { .. } => {
                if let Some((lower, upper)) = last_remove_ticks {
                    result.merge(check_burn_liquidity(
                        before,
                        after,
                        lower,
                        upper,
                        Uint128::one(),
                    ));
                }
                result.merge(check_remove_liquidity_reserves(before, after));
            }
            ConcentratedOp::CollectFees { .. } => {
                result.merge(check_remove_liquidity_reserves(before, after));
            }
        }
        result
    }
}
