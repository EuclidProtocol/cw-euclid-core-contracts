mod post_test;
mod snapshot;
mod transition;

// Re-export snapshot checks (used directly by boundary tests)
pub use post_test::*;
pub use snapshot::*;

use super::shared::check_clean_removal;
use super::{InvariantCheck, InvariantResult, PoolSnapshot};

// =============================================================================
// COMPOSITE CHECKERS
// =============================================================================

/// Run cheap snapshot invariants that only need slot0 data (no positions/ticks).
/// Used for per-op checks in timed runs to avoid O(n) position enumeration.
pub fn check_light_snapshot(snapshot: &PoolSnapshot) -> InvariantResult {
    let mut result = InvariantResult::new();
    result.add(check_tick_price_consistency(snapshot));
    result.add(check_sqrt_price_bounds(snapshot));
    // Lightweight solvency: reserves must at least cover protocol fees
    // (full CL:snap:reserve_solvency also checks tokens_owed but that requires position data)
    if snapshot.reserve_0.u128() < snapshot.protocol_fees.amount_0.u128() {
        result.add(InvariantCheck::fail(
            "CL:snap:protocol_fee_coverage_0",
            format!(
                "reserve_0 ({}) < protocol_fees_0 ({})",
                snapshot.reserve_0, snapshot.protocol_fees.amount_0
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("CL:snap:protocol_fee_coverage_0"));
    }
    if snapshot.reserve_1.u128() < snapshot.protocol_fees.amount_1.u128() {
        result.add(InvariantCheck::fail(
            "CL:snap:protocol_fee_coverage_1",
            format!(
                "reserve_1 ({}) < protocol_fees_1 ({})",
                snapshot.reserve_1, snapshot.protocol_fees.amount_1
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("CL:snap:protocol_fee_coverage_1"));
    }
    result
}

/// Run all snapshot invariants on current state
pub fn check_all_snapshot_invariants(
    snapshot: &PoolSnapshot,
    tick_spacing: u64,
) -> InvariantResult {
    let mut result = InvariantResult::new();
    result.add(check_tick_price_consistency(snapshot));
    result.add(check_active_liquidity(snapshot));
    result.merge(check_tick_liquidity_gross(snapshot));
    result.add(check_liquidity_net_sum_zero(snapshot));
    result.merge(check_position_bounds(snapshot, tick_spacing));
    result.merge(check_fee_growth_inside_consistency(snapshot));
    result.add(check_sqrt_price_bounds(snapshot));
    result.merge(check_reserve_solvency(snapshot));
    result
}

/// Run all post-test assertions (both CL-specific and shared)
pub fn check_post_test(snapshot: &PoolSnapshot) -> InvariantResult {
    let mut result = InvariantResult::new();
    result.add(check_clean_ticks(snapshot));
    result.add(check_clean_removal(snapshot));
    result
}
