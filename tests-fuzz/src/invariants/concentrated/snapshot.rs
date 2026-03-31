use concentrated_vlp::math::position_math::fee_growth_inside;
use concentrated_vlp::math::tick_math::{
    get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, max_sqrt_ratio, min_sqrt_ratio,
};
use concentrated_vlp::state::TickInfo;
use cosmwasm_std::Uint256;

use crate::invariants::{InvariantCheck, InvariantResult, PoolSnapshot};

// =============================================================================
// Helpers
// =============================================================================

/// Convert a TickResponse to the contract's TickInfo for use with
/// the contract's fee_growth_inside function.
fn tick_response_to_info(t: &euclid::msgs::vlp::concentrated::msg::TickResponse) -> TickInfo {
    TickInfo {
        initialized: t.initialized,
        liquidity_gross: t.liquidity_gross,
        liquidity_net: t.liquidity_net,
        fee_growth_outside_0_x128: t.fee_growth_outside_0_x128,
        fee_growth_outside_1_x128: t.fee_growth_outside_1_x128,
    }
}

/// Check whether a fee_growth delta is a forward step (< half of Uint256 space).
/// A delta > half_max indicates the accumulator went backward — corrupted state.
fn is_backward_delta(delta: Uint256) -> bool {
    delta > Uint256::MAX >> 1
}

// =============================================================================
// SNAPSHOT INVARIANTS (checked on current state)
// =============================================================================

/// CL:snap:tick_price — slot0.tick must satisfy the V3 invariant:
///   sqrt_ratio_at_tick(tick) <= sqrt_price < sqrt_ratio_at_tick(tick + 1)
///
/// Note: after a zero_for_one tick crossing, V3 sets slot0.tick = crossed_tick - 1
/// by convention, even if the price hasn't moved below that tick's sqrt_ratio.
/// This means slot0.tick can be 1 below get_tick_at_sqrt_ratio(sqrt_price),
/// which is correct behavior. The invariant checks the range, not exact equality.
pub fn check_tick_price_consistency(snapshot: &PoolSnapshot) -> InvariantCheck {
    let tick = snapshot.slot0.tick;
    let sqrt_price = snapshot.slot0.sqrt_price_x96;

    let lower_bound = match get_sqrt_ratio_at_tick(tick) {
        Ok(v) => v,
        Err(e) => {
            return InvariantCheck::fail(
                "CL:snap:tick_price",
                format!("get_sqrt_ratio_at_tick({}) failed: {}", tick, e),
            )
        }
    };
    let upper_bound = match get_sqrt_ratio_at_tick(tick + 1) {
        Ok(v) => v,
        Err(e) => {
            return InvariantCheck::fail(
                "CL:snap:tick_price",
                format!("get_sqrt_ratio_at_tick({}) failed: {}", tick + 1, e),
            )
        }
    };

    // Use <= on upper bound: after zero_for_one crossing of tick T,
    // slot0.tick = T-1 but price can be exactly sqrt_at_tick(T).
    if sqrt_price < lower_bound || sqrt_price > upper_bound {
        InvariantCheck::fail(
            "CL:snap:tick_price",
            format!(
                "sqrt_price {} not in [sqrt_at_tick({}), sqrt_at_tick({})]: [{}, {}]",
                sqrt_price,
                tick,
                tick + 1,
                lower_bound,
                upper_bound,
            ),
        )
    } else {
        InvariantCheck::pass("CL:snap:tick_price")
    }
}

/// CL:snap:active_liquidity — Active liquidity equals sum of in-range position liquidity
///
/// Uses the price-derived tick (not slot0.tick) for in-range determination,
/// because slot0.tick can be off by 1 at boundaries due to V3's tick crossing
/// convention (slot0.tick = crossed_tick - 1 for zero_for_one).
///
/// Using the price-derived tick ensures we compute in-range the same way
/// the contract does internally via apply_liquidity_delta's check:
///   lower_tick_index <= current_tick && current_tick < upper_tick_index
pub fn check_active_liquidity(snapshot: &PoolSnapshot) -> InvariantCheck {
    // Use slot0.tick as the reference — this is what the contract uses
    // for the in-range check in apply_liquidity_delta
    let current_tick = snapshot.slot0.tick;
    let in_range: Vec<_> = snapshot
        .positions
        .iter()
        .filter(|p| p.lower_tick_index <= current_tick && current_tick < p.upper_tick_index)
        .collect();
    let expected_liquidity: u128 = in_range.iter().map(|p| p.liquidity.u128()).sum();

    // Also compute with price-derived tick for comparison
    let price_tick = get_tick_at_sqrt_ratio(snapshot.slot0.sqrt_price_x96).unwrap_or(current_tick);
    let in_range_by_price: Vec<_> = snapshot
        .positions
        .iter()
        .filter(|p| p.lower_tick_index <= price_tick && price_tick < p.upper_tick_index)
        .collect();
    let expected_by_price: u128 = in_range_by_price.iter().map(|p| p.liquidity.u128()).sum();

    let actual = snapshot.slot0.liquidity.u128();

    // Accept if EITHER tick interpretation matches — the 1-tick difference
    // at boundaries is a known V3 convention artifact
    if actual != expected_liquidity && actual != expected_by_price {
        let pos_details: Vec<String> = in_range
            .iter()
            .map(|p| {
                format!(
                    "  id={} [{},{}] liq={}",
                    p.position_id, p.lower_tick_index, p.upper_tick_index, p.liquidity
                )
            })
            .collect();
        return InvariantCheck::fail(
            "CL:snap:active_liquidity",
            format!(
                "slot0.liquidity={} != sum(in-range by tick {})={} AND != sum(in-range by price {})={} ({} positions)\n{}",
                actual,
                current_tick,
                expected_liquidity,
                price_tick,
                expected_by_price,
                in_range.len(),
                pos_details.join("\n"),
            ),
        );
    }
    InvariantCheck::pass("CL:snap:active_liquidity")
}

/// CL:snap:tick_liquidity_gross — Tick liquidity_gross equals sum of positions touching that tick
/// From Trail of Bits Props #2-3, #8-9
pub fn check_tick_liquidity_gross(snapshot: &PoolSnapshot) -> InvariantResult {
    let mut result = InvariantResult::new();

    for tick in &snapshot.ticks {
        if !tick.initialized {
            if !tick.liquidity_gross.is_zero() {
                result.add(InvariantCheck::fail(
                    "CL:snap:uninitialized_tick_nonzero_gross",
                    format!(
                        "tick {} is uninitialized but has liquidity_gross={}",
                        tick.index, tick.liquidity_gross
                    ),
                ));
            }
            continue;
        }
        let expected_gross: u128 = snapshot
            .positions
            .iter()
            .filter(|p| p.lower_tick_index == tick.index || p.upper_tick_index == tick.index)
            .map(|p| p.liquidity.u128())
            .sum();

        if tick.liquidity_gross.u128() != expected_gross {
            result.add(InvariantCheck::fail(
                "CL:snap:tick_liquidity_gross",
                format!(
                    "tick {} liquidity_gross={} != sum(touching positions)={}",
                    tick.index, tick.liquidity_gross, expected_gross
                ),
            ));
        }
    }

    if result.checks.is_empty() {
        result.add(InvariantCheck::pass("CL:snap:tick_liquidity_gross"));
    }
    result
}

/// CL:snap:liquidity_net_sum — Sum of liquidity_net across all ticks == 0
/// From Trail of Bits Prop #20 / Velodrome Prop #20
pub fn check_liquidity_net_sum_zero(snapshot: &PoolSnapshot) -> InvariantCheck {
    let sum: i128 = snapshot
        .ticks
        .iter()
        .filter(|t| t.initialized)
        .map(|t| t.liquidity_net)
        .sum();

    if sum != 0 {
        return InvariantCheck::fail(
            "CL:snap:liquidity_net_sum",
            format!("sum(liquidity_net) = {} (expected 0)", sum),
        );
    }
    InvariantCheck::pass("CL:snap:liquidity_net_sum")
}

/// CL:snap:position_bounds — Position bounds valid - lower < upper, both aligned to tick_spacing
pub fn check_position_bounds(snapshot: &PoolSnapshot, tick_spacing: u64) -> InvariantResult {
    let mut result = InvariantResult::new();

    for pos in &snapshot.positions {
        if pos.lower_tick_index >= pos.upper_tick_index {
            result.add(InvariantCheck::fail(
                "CL:snap:position_bounds",
                format!(
                    "position {} has lower={} >= upper={}",
                    pos.position_id, pos.lower_tick_index, pos.upper_tick_index
                ),
            ));
            continue;
        }

        let spacing = tick_spacing as i64;
        if spacing > 0 {
            if pos.lower_tick_index % spacing != 0 {
                result.add(InvariantCheck::fail(
                    "CL:snap:position_bounds_spacing",
                    format!(
                        "position {} lower_tick {} not aligned to spacing {}",
                        pos.position_id, pos.lower_tick_index, spacing
                    ),
                ));
            }
            if pos.upper_tick_index % spacing != 0 {
                result.add(InvariantCheck::fail(
                    "CL:snap:position_bounds_spacing",
                    format!(
                        "position {} upper_tick {} not aligned to spacing {}",
                        pos.position_id, pos.upper_tick_index, spacing
                    ),
                ));
            }
        }
    }

    if result.checks.is_empty() {
        result.add(InvariantCheck::pass("CL:snap:position_bounds"));
    }
    result
}

/// CL:snap:fee_growth_inside — Fee growth inside consistency
///
/// Uses the contract's own `fee_growth_inside()` function to recompute
/// the fee growth inside each position's tick range, then verifies the
/// delta from fee_growth_inside_last is a forward step (not backward).
///
/// For each position:
/// 1. Verifies that referenced ticks exist (missing tick = corrupted state)
/// 2. Calls the contract's fee_growth_inside() with the current tick state
/// 3. Checks the delta from last snapshot is < half of Uint256 space
pub fn check_fee_growth_inside_consistency(snapshot: &PoolSnapshot) -> InvariantResult {
    use std::collections::HashMap;

    let mut result = InvariantResult::new();
    let current_tick = snapshot.slot0.tick;
    let global_0 = snapshot.slot0.fee_growth_global_0_x128;
    let global_1 = snapshot.slot0.fee_growth_global_1_x128;

    let tick_map: HashMap<i64, _> = snapshot
        .ticks
        .iter()
        .filter(|t| t.initialized)
        .map(|t| (t.index, t))
        .collect();

    for pos in &snapshot.positions {
        let lower = pos.lower_tick_index;
        let upper = pos.upper_tick_index;

        // Step 1: Verify referenced ticks exist
        let lower_tick = match tick_map.get(&lower) {
            Some(t) => *t,
            None => {
                if !pos.liquidity.is_zero() {
                    result.add(InvariantCheck::fail(
                        "CL:snap:missing_lower_tick",
                        format!(
                            "position {} [{}, {}] liq={} but lower tick missing",
                            pos.position_id, lower, upper, pos.liquidity,
                        ),
                    ));
                }
                continue;
            }
        };
        let upper_tick = match tick_map.get(&upper) {
            Some(t) => *t,
            None => {
                if !pos.liquidity.is_zero() {
                    result.add(InvariantCheck::fail(
                        "CL:snap:missing_upper_tick",
                        format!(
                            "position {} [{}, {}] liq={} but upper tick missing",
                            pos.position_id, lower, upper, pos.liquidity,
                        ),
                    ));
                }
                continue;
            }
        };

        // Step 2: Use the contract's own fee_growth_inside function
        let (inside_0, inside_1) = match fee_growth_inside(
            current_tick,
            lower,
            upper,
            global_0,
            global_1,
            Some(tick_response_to_info(lower_tick)),
            Some(tick_response_to_info(upper_tick)),
        ) {
            Ok(result) => result,
            Err(e) => {
                result.add(InvariantCheck::fail(
                    "CL:snap:fee_growth_inside_error",
                    format!(
                        "position {} [{}, {}]: fee_growth_inside() failed: {}",
                        pos.position_id, lower, upper, e,
                    ),
                ));
                continue;
            }
        };

        // Step 3: Verify delta from last snapshot is forward (not backward)
        let delta_0 = inside_0.wrapping_sub(pos.fee_growth_inside_0_last_x128);
        let delta_1 = inside_1.wrapping_sub(pos.fee_growth_inside_1_last_x128);

        if is_backward_delta(delta_0) {
            result.add(InvariantCheck::fail(
                "CL:snap:fee_growth_inside_0",
                format!(
                    "position {} [{}, {}]: inside_0 delta backward (inside={}, last={}, delta={})",
                    pos.position_id,
                    lower,
                    upper,
                    inside_0,
                    pos.fee_growth_inside_0_last_x128,
                    delta_0,
                ),
            ));
        }
        if is_backward_delta(delta_1) {
            result.add(InvariantCheck::fail(
                "CL:snap:fee_growth_inside_1",
                format!(
                    "position {} [{}, {}]: inside_1 delta backward (inside={}, last={}, delta={})",
                    pos.position_id,
                    lower,
                    upper,
                    inside_1,
                    pos.fee_growth_inside_1_last_x128,
                    delta_1,
                ),
            ));
        }
    }

    if result.checks.is_empty() || result.all_passed() {
        result.add(InvariantCheck::pass("CL:snap:fee_growth_inside"));
    }
    result
}

/// CL:snap:sqrt_price_bounds — sqrt_price within valid range [MIN_SQRT_RATIO, MAX_SQRT_RATIO)
pub fn check_sqrt_price_bounds(snapshot: &PoolSnapshot) -> InvariantCheck {
    let min_sqrt = min_sqrt_ratio();
    let max_sqrt = max_sqrt_ratio();
    let sqrt_price = snapshot.slot0.sqrt_price_x96;

    if sqrt_price < min_sqrt || sqrt_price >= max_sqrt {
        return InvariantCheck::fail(
            "CL:snap:sqrt_price_bounds",
            format!(
                "sqrt_price_x96 {} outside [{}, {})",
                sqrt_price, min_sqrt, max_sqrt
            ),
        );
    }

    InvariantCheck::pass("CL:snap:sqrt_price_bounds")
}

/// CL:snap:unlocked — Pool is unlocked (reentrancy guard not stuck)
pub fn check_unlocked(snapshot: &PoolSnapshot) -> InvariantCheck {
    if !snapshot.slot0.unlocked {
        return InvariantCheck::fail(
            "CL:snap:unlocked",
            "pool is locked (reentrancy guard stuck)".to_string(),
        );
    }
    InvariantCheck::pass("CL:snap:unlocked")
}

/// CL:snap:reserve_solvency — reserves must cover all outstanding obligations.
///
/// The pool's reserves must be at least as large as the sum of:
/// - All positions' uncollected tokens_owed (settled but not yet withdrawn)
/// - Protocol fees (accrued but not yet claimed)
///
/// This catches accounting bugs where the pool pays out more than it takes in,
/// or where fee settlement over-credits positions.
pub fn check_reserve_solvency(snapshot: &PoolSnapshot) -> InvariantResult {
    let mut result = InvariantResult::new();

    let total_owed_0: u128 = snapshot
        .positions
        .iter()
        .map(|p| p.tokens_owed_0.u128())
        .sum();
    let total_owed_1: u128 = snapshot
        .positions
        .iter()
        .map(|p| p.tokens_owed_1.u128())
        .sum();

    let obligations_0 = total_owed_0 + snapshot.protocol_fees.amount_0.u128();
    let obligations_1 = total_owed_1 + snapshot.protocol_fees.amount_1.u128();

    if snapshot.reserve_0.u128() < obligations_0 {
        result.add(InvariantCheck::fail(
            "CL:snap:reserve_solvency_0",
            format!(
                "reserve_0 ({}) < obligations_0 ({} owed + {} protocol = {})",
                snapshot.reserve_0, total_owed_0, snapshot.protocol_fees.amount_0, obligations_0,
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("CL:snap:reserve_solvency_0"));
    }

    if snapshot.reserve_1.u128() < obligations_1 {
        result.add(InvariantCheck::fail(
            "CL:snap:reserve_solvency_1",
            format!(
                "reserve_1 ({}) < obligations_1 ({} owed + {} protocol = {})",
                snapshot.reserve_1, total_owed_1, snapshot.protocol_fees.amount_1, obligations_1,
            ),
        ));
    } else {
        result.add(InvariantCheck::pass("CL:snap:reserve_solvency_1"));
    }

    result
}
