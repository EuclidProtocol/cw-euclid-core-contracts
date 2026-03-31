use cosmwasm_std::Uint128;
use rand::rngs::StdRng;
use rand::Rng;

use crate::harness::concentrated::ConcentratedConfig;

/// Live pool state passed to strategy for smarter op generation
#[derive(Debug, Clone)]
pub struct PoolState {
    pub current_tick: i64,
    pub reserve_0: Uint128,
    pub reserve_1: Uint128,
}

/// Operation types for concentrated liquidity fuzz testing.
///
/// Each variant carries a `user_idx` selecting which user performs the action.
/// Index 0 is the default user; single-user tests and the `FuzzRunner` always
/// use 0. Multi-user tests set this to route operations to different senders.
#[derive(Debug, Clone)]
pub enum ConcentratedOp {
    Swap {
        amount: u128,
        zero_for_one: bool,
        user_idx: usize,
    },
    AddLiquidity {
        lower_tick: i64,
        upper_tick: i64,
        amount_0: u128,
        amount_1: u128,
        user_idx: usize,
    },
    RemoveLiquidity {
        position_idx: usize,
        fraction_bps: u64,
        user_idx: usize,
    },
    CollectFees {
        position_idx: usize,
        user_idx: usize,
    },
}

impl ConcentratedOp {
    /// Return a short string label for this operation type.
    pub fn name(&self) -> &'static str {
        match self {
            ConcentratedOp::Swap { .. } => "swap",
            ConcentratedOp::AddLiquidity { .. } => "add_liquidity",
            ConcentratedOp::RemoveLiquidity { .. } => "remove_liquidity",
            ConcentratedOp::CollectFees { .. } => "collect_fees",
        }
    }

    /// Get the user index for this operation.
    pub fn user_idx(&self) -> usize {
        match self {
            ConcentratedOp::Swap { user_idx, .. }
            | ConcentratedOp::AddLiquidity { user_idx, .. }
            | ConcentratedOp::RemoveLiquidity { user_idx, .. }
            | ConcentratedOp::CollectFees { user_idx, .. } => *user_idx,
        }
    }

    /// Return a copy with the user index overridden.
    pub fn with_user_idx(self, idx: usize) -> Self {
        match self {
            ConcentratedOp::Swap {
                amount,
                zero_for_one,
                ..
            } => ConcentratedOp::Swap {
                amount,
                zero_for_one,
                user_idx: idx,
            },
            ConcentratedOp::AddLiquidity {
                lower_tick,
                upper_tick,
                amount_0,
                amount_1,
                ..
            } => ConcentratedOp::AddLiquidity {
                lower_tick,
                upper_tick,
                amount_0,
                amount_1,
                user_idx: idx,
            },
            ConcentratedOp::RemoveLiquidity {
                position_idx,
                fraction_bps,
                ..
            } => ConcentratedOp::RemoveLiquidity {
                position_idx,
                fraction_bps,
                user_idx: idx,
            },
            ConcentratedOp::CollectFees { position_idx, .. } => ConcentratedOp::CollectFees {
                position_idx,
                user_idx: idx,
            },
        }
    }

    /// Generate a random operation using seeded RNG, config weights, and live pool state.
    ///
    /// When position count exceeds `config.max_positions`, 80% of ops become full
    /// removes to prevent unbounded position growth and O(n) snapshot degradation.
    pub fn random(
        rng: &mut StdRng,
        num_positions: usize,
        config: &ConcentratedConfig,
        state: &PoolState,
    ) -> Self {
        // Position cap: when too many positions exist, heavily bias toward full removes
        if num_positions > config.max_positions && num_positions > 0 && rng.gen_bool(0.8) {
            let position_idx = rng.gen_range(0..num_positions);
            return ConcentratedOp::RemoveLiquidity {
                position_idx,
                fraction_bps: 10_000, // Full remove
                user_idx: 0,
            };
        }

        let op_type: u32 = rng.gen_range(0..100);

        let swap_bound = config.weight_swap;
        let add_bound = swap_bound + config.weight_add;
        let remove_bound = add_bound + config.weight_remove;

        if op_type < swap_bound {
            Self::random_swap(rng, config, state)
        } else if op_type < add_bound {
            Self::random_add(rng, config, state)
        } else if op_type < remove_bound {
            Self::random_remove(rng, num_positions, config)
        } else {
            Self::random_collect(rng, num_positions, config)
        }
    }

    /// Generate a swap biased toward the direction with reserves.
    fn random_swap(rng: &mut StdRng, config: &ConcentratedConfig, state: &PoolState) -> Self {
        let amount = rng.gen_range(config.swap_amount_range.0..config.swap_amount_range.1);

        let has_reserve_0 = !state.reserve_0.is_zero();
        let has_reserve_1 = !state.reserve_1.is_zero();

        // zero_for_one = true means selling token0, buying token1 (takes from reserve_1)
        // zero_for_one = false means selling token1, buying token0 (takes from reserve_0)
        let zero_for_one = match (has_reserve_0, has_reserve_1) {
            (true, true) => rng.gen_bool(0.5),
            (false, true) => true,  // only reserve_1 available to take from
            (true, false) => false, // only reserve_0 available to take from
            (false, false) => rng.gen_bool(0.5),
        };

        // Usually cap swap to 90% of output reserves to avoid constant failures,
        // but 10% of the time allow uncapped swaps to test reserve exhaustion.
        let max_output = if zero_for_one {
            state.reserve_1
        } else {
            state.reserve_0
        };
        let amount = if !max_output.is_zero() && rng.gen_bool(0.9) {
            amount
                .min(max_output.u128() * 9 / 10)
                .max(config.swap_amount_range.0)
        } else {
            amount
        };

        ConcentratedOp::Swap {
            amount,
            zero_for_one,
            user_idx: 0,
        }
    }

    /// Generate an add_liquidity op with tick-aware amounts.
    ///
    /// Concentrated liquidity math (Uniswap V3):
    /// - Position below current tick (upper_tick ≤ current): only token1 used
    /// - Position above current tick (lower_tick ≥ current): only token0 used
    /// - Spanning current tick: both tokens used, ratio depends on position within range
    ///
    /// 80% of the time we generate in-range positions (spanning current tick) for
    /// better success rate. Both amounts are always ≥ 1 (contract requirement).
    fn random_add(rng: &mut StdRng, config: &ConcentratedConfig, state: &PoolState) -> Self {
        let spacing = config.tick_spacing as i64;
        let min_tick = (config.tick_range.0 / spacing) * spacing;
        let max_tick = (config.tick_range.1 / spacing) * spacing;
        let current_tick = state.current_tick;
        let range = config.add_amount_range;

        // 80% in-range (spanning current tick), 20% random
        let force_in_range = rng.gen_bool(0.8);

        let (lower_tick, upper_tick) = if force_in_range {
            gen_in_range_ticks(rng, min_tick, max_tick, current_tick, spacing)
        } else {
            gen_random_ticks(rng, min_tick, max_tick, spacing)
        };

        // Generate amounts based on position relative to current tick.
        // Contract requires both > 0, so floor at 1.
        let (amount_0, amount_1) = if upper_tick <= current_tick {
            // Below current tick → sqrt_p ≥ sqrt_b → only token1 used, token0 refunded
            (1u128, rng.gen_range(range.0..range.1))
        } else if lower_tick >= current_tick {
            // Above current tick → sqrt_p ≤ sqrt_a → only token0 used, token1 refunded
            (rng.gen_range(range.0..range.1), 1u128)
        } else {
            // In-range: both tokens used. Scale proportionally using integer math.
            // frac_above = fraction of range above current → needs token0
            // frac_below = fraction of range below current → needs token1
            // Safe: lower < current < upper for in-range, so diffs are positive.
            let total_range = (upper_tick - lower_tick) as u128;
            let base = rng.gen_range(range.0..range.1);
            let a0 = base * (upper_tick - current_tick) as u128 / total_range;
            let a1 = base * (current_tick - lower_tick) as u128 / total_range;
            // Floor at 1 to satisfy contract constraint
            (a0.max(1), a1.max(1))
        };

        ConcentratedOp::AddLiquidity {
            lower_tick,
            upper_tick,
            amount_0,
            amount_1,
            user_idx: 0,
        }
    }

    /// Fallback when no positions exist: add liquidity centered on tick 0
    fn fallback_add(rng: &mut StdRng, config: &ConcentratedConfig) -> Self {
        let spacing = config.tick_spacing as i64;
        let amount = rng.gen_range(config.add_amount_range.0..config.add_amount_range.1);
        ConcentratedOp::AddLiquidity {
            lower_tick: -spacing,
            upper_tick: spacing,
            amount_0: amount,
            amount_1: amount,
            user_idx: 0,
        }
    }

    /// Generate a remove_liquidity op for a random position, or fallback to add if none exist.
    fn random_remove(rng: &mut StdRng, num_positions: usize, config: &ConcentratedConfig) -> Self {
        if num_positions == 0 {
            return Self::fallback_add(rng, config);
        }
        let position_idx = rng.gen_range(0..num_positions);
        let fraction_bps =
            rng.gen_range(config.remove_fraction_bps_range.0..=config.remove_fraction_bps_range.1);
        ConcentratedOp::RemoveLiquidity {
            position_idx,
            fraction_bps,
            user_idx: 0,
        }
    }

    /// Generate a collect_fees op for a random position, or fallback to add if none exist.
    fn random_collect(rng: &mut StdRng, num_positions: usize, config: &ConcentratedConfig) -> Self {
        if num_positions == 0 {
            return Self::fallback_add(rng, config);
        }
        let position_idx = rng.gen_range(0..num_positions);
        ConcentratedOp::CollectFees {
            position_idx,
            user_idx: 0,
        }
    }

    /// Generate a swap targeting near the next tick boundary
    pub fn near_boundary_swap(rng: &mut StdRng, tick_spacing: u64) -> Self {
        let base_amount = tick_spacing as u128 * 100;
        let jitter: i64 = rng.gen_range(-50..50);
        let amount = (base_amount as i64 + jitter).unsigned_abs() as u128;
        let amount = amount.max(10);
        let zero_for_one = rng.gen_bool(0.5);
        ConcentratedOp::Swap {
            amount,
            zero_for_one,
            user_idx: 0,
        }
    }

    /// Generate a swap with dust-level amounts (Balancer V2 exploit pattern)
    pub fn dust_swap(rng: &mut StdRng) -> Self {
        let amount = rng.gen_range(1..10u128);
        let zero_for_one = rng.gen_bool(0.5);
        ConcentratedOp::Swap {
            amount,
            zero_for_one,
            user_idx: 0,
        }
    }
}

/// Generate tick range guaranteed to span the current tick (in-range position).
pub fn gen_in_range_ticks(
    rng: &mut StdRng,
    min_tick: i64,
    max_tick: i64,
    current_tick: i64,
    spacing: i64,
) -> (i64, i64) {
    // Align current tick down to spacing
    let current_aligned = (current_tick / spacing) * spacing;

    // Lower tick: between min_tick and current_aligned (at least one spacing below)
    let lower_bound = min_tick;
    let lower_upper = (current_aligned - spacing).max(min_tick);
    let lower_tick = if lower_bound < lower_upper {
        let num_options = (lower_upper - lower_bound) / spacing + 1;
        let idx = rng.gen_range(0..num_options);
        lower_bound + idx * spacing
    } else {
        lower_bound
    };

    // Upper tick: between current_aligned + spacing and max_tick
    let upper_lower = (current_aligned + spacing).min(max_tick);
    let upper_bound = max_tick;
    let upper_tick = if upper_lower < upper_bound {
        let num_options = (upper_bound - upper_lower) / spacing + 1;
        let idx = rng.gen_range(0..num_options);
        upper_lower + idx * spacing
    } else {
        upper_bound
    };

    // Safety: ensure lower < upper
    if lower_tick >= upper_tick {
        // Fallback to full range
        return (min_tick, max_tick);
    }

    (lower_tick, upper_tick)
}

/// Generate fully random aligned ticks (may be out of range).
fn gen_random_ticks(rng: &mut StdRng, min_tick: i64, max_tick: i64, spacing: i64) -> (i64, i64) {
    let num_spacings = ((max_tick - min_tick) / spacing).max(2);
    let lower_idx = rng.gen_range(0..num_spacings - 1);
    let upper_idx = rng.gen_range(lower_idx + 1..num_spacings);
    (
        min_tick + lower_idx * spacing,
        min_tick + upper_idx * spacing,
    )
}
