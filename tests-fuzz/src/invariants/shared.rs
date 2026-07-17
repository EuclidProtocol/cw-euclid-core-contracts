use cosmwasm_std::Uint256;

use super::{InvariantCheck, InvariantResult, PoolSnapshot};

/// Returns true if a Uint256 "decrease" is actually a wrapping-math forward step.
/// fee_growth accumulators use modular arithmetic (mod 2^256) — a decrease that
/// spans more than half the Uint256 space is actually a forward wrap, not a
/// real decrease. A real decrease would be a small backward step.
pub fn is_wrapping_decrease(before: Uint256, after: Uint256) -> bool {
    if after >= before {
        return false; // not a decrease at all
    }
    // delta = before - after. If delta > MAX/2, it's a wrap (forward step).
    let delta = before - after;
    delta > Uint256::MAX >> 1
}

/// S:txn:fee_growth_monotonic — Fee growth is monotonically non-decreasing (transition check)
///
/// Accounts for wrapping arithmetic: fee_growth accumulators can wrap past
/// Uint256::MAX. A "decrease" that spans more than half the Uint256 space
/// is actually a forward wrap, not a real decrease.
pub fn check_fee_monotonicity(before: &PoolSnapshot, after: &PoolSnapshot) -> InvariantResult {
    let mut result = InvariantResult::new();

    let g0_before = before.slot0.fee_growth_global_0_x128;
    let g0_after = after.slot0.fee_growth_global_0_x128;
    if g0_after < g0_before && !is_wrapping_decrease(g0_before, g0_after) {
        result.add(InvariantCheck::fail(
            "S:txn:fee_growth_0_monotonic",
            format!(
                "fee_growth_global_0 decreased (not a wrap): {} -> {}",
                g0_before, g0_after
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("S:txn:fee_growth_0_monotonic"));
    }

    let g1_before = before.slot0.fee_growth_global_1_x128;
    let g1_after = after.slot0.fee_growth_global_1_x128;
    if g1_after < g1_before && !is_wrapping_decrease(g1_before, g1_after) {
        result.add(InvariantCheck::fail(
            "S:txn:fee_growth_1_monotonic",
            format!(
                "fee_growth_global_1 decreased (not a wrap): {} -> {}",
                g1_before, g1_after
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("S:txn:fee_growth_1_monotonic"));
    }

    result
}

/// S:txn:protocol_fees_monotonic — Protocol fees are monotonically non-decreasing (transition check)
pub fn check_protocol_fees_monotonic(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
) -> InvariantResult {
    let mut result = InvariantResult::new();

    if after.protocol_fees.amount_0 < before.protocol_fees.amount_0 {
        result.add(InvariantCheck::fail(
            "S:txn:protocol_fees_0_monotonic",
            format!(
                "protocol_fees_0 decreased: {} -> {}",
                before.protocol_fees.amount_0, after.protocol_fees.amount_0
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("S:txn:protocol_fees_0_monotonic"));
    }

    if after.protocol_fees.amount_1 < before.protocol_fees.amount_1 {
        result.add(InvariantCheck::fail(
            "S:txn:protocol_fees_1_monotonic",
            format!(
                "protocol_fees_1 decreased: {} -> {}",
                before.protocol_fees.amount_1, after.protocol_fees.amount_1
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("S:txn:protocol_fees_1_monotonic"));
    }

    result
}

/// S:snap:reserves_consistent — If the pool has active liquidity, at least one reserve must be non-zero.
/// A pool with positions but empty reserves indicates a drain or accounting bug.
pub fn check_reserves_consistent(snapshot: &PoolSnapshot) -> InvariantCheck {
    if !snapshot.slot0.liquidity.is_zero()
        && snapshot.reserve_0.is_zero()
        && snapshot.reserve_1.is_zero()
    {
        return InvariantCheck::fail(
            "S:snap:reserves_consistent",
            format!(
                "pool has active liquidity ({}) but both reserves are zero",
                snapshot.slot0.liquidity
            ),
        );
    }
    InvariantCheck::pass("S:snap:reserves_consistent")
}

/// S:txn:add_reserves_non_decreasing — Add liquidity reserve conservation.
/// Both reserves must be >= before (tokens flow in, not out).
pub fn check_add_liquidity_reserves(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
) -> InvariantResult {
    let mut result = InvariantResult::new();

    if after.reserve_0 < before.reserve_0 {
        result.add(InvariantCheck::fail(
            "S:txn:add_reserve_0_non_decreasing",
            format!(
                "reserve_0 decreased on add: {} -> {}",
                before.reserve_0, after.reserve_0
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("S:txn:add_reserve_0_non_decreasing"));
    }

    if after.reserve_1 < before.reserve_1 {
        result.add(InvariantCheck::fail(
            "S:txn:add_reserve_1_non_decreasing",
            format!(
                "reserve_1 decreased on add: {} -> {}",
                before.reserve_1, after.reserve_1
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("S:txn:add_reserve_1_non_decreasing"));
    }

    result
}

/// S:txn:remove_reserves_non_increasing — Remove/collect reserves.
/// Reserves can only decrease (tokens flow out).
/// Protocol fees must not decrease (remove doesn't touch protocol fees).
pub fn check_remove_liquidity_reserves(
    before: &PoolSnapshot,
    after: &PoolSnapshot,
) -> InvariantResult {
    let mut result = InvariantResult::new();

    if after.reserve_0 > before.reserve_0 {
        result.add(InvariantCheck::fail(
            "S:txn:remove_reserve_0_non_increasing",
            format!(
                "reserve_0 increased on remove: {} -> {}",
                before.reserve_0, after.reserve_0
            ),
        ));
    } else {
        result.add(InvariantCheck::pass(
            "S:txn:remove_reserve_0_non_increasing",
        ));
    }

    if after.reserve_1 > before.reserve_1 {
        result.add(InvariantCheck::fail(
            "S:txn:remove_reserve_1_non_increasing",
            format!(
                "reserve_1 increased on remove: {} -> {}",
                before.reserve_1, after.reserve_1
            ),
        ));
    } else {
        result.add(InvariantCheck::pass(
            "S:txn:remove_reserve_1_non_increasing",
        ));
    }

    result
}

/// S:post:clean_removal — After removing all positions, pool liquidity is zero
pub fn check_clean_removal(snapshot: &PoolSnapshot) -> InvariantCheck {
    if !snapshot.slot0.liquidity.is_zero() {
        return InvariantCheck::fail(
            "S:post:clean_removal",
            format!(
                "liquidity not zero after full removal: {}",
                snapshot.slot0.liquidity
            ),
        );
    }
    InvariantCheck::pass("S:post:clean_removal")
}

/// Run all shared snapshot invariants
pub fn check_shared_snapshot(snapshot: &PoolSnapshot) -> InvariantResult {
    let mut result = InvariantResult::new();
    result.add(check_reserves_consistent(snapshot));
    result
}

/// Run all shared transition invariants
pub fn check_shared_transition(before: &PoolSnapshot, after: &PoolSnapshot) -> InvariantResult {
    let mut result = InvariantResult::new();
    result.merge(check_fee_monotonicity(before, after));
    result.merge(check_protocol_fees_monotonic(before, after));
    result
}
