use cosmwasm_std::Uint128;

use crate::invariants::{InvariantCheck, PoolSnapshot};

// =============================================================================
// POST-TEST ASSERTIONS (after all operations complete)
// =============================================================================

/// CL:post:clean_ticks — All tick liquidity_net sums to zero after full removal
pub fn check_clean_ticks(snapshot: &PoolSnapshot) -> InvariantCheck {
    let has_initialized = snapshot
        .ticks
        .iter()
        .any(|t| t.initialized && t.liquidity_gross > Uint128::zero());
    if has_initialized {
        return InvariantCheck::fail(
            "CL:post:clean_ticks",
            "initialized ticks with non-zero liquidity_gross remain after full removal".to_string(),
        );
    }
    InvariantCheck::pass("CL:post:clean_ticks")
}
