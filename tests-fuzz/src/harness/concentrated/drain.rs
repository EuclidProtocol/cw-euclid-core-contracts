use rand::rngs::StdRng;
use rand::Rng;

use crate::runner::FuzzPool;
use crate::strategies::concentrated::{gen_in_range_ticks, ConcentratedOp};

use super::ConcentratedPool;

impl ConcentratedPool {
    /// Seed initial liquidity spread across all users (positions_per_user per user).
    /// When only one user exists, this is equivalent to single-user seeding.
    pub(crate) fn seed_positions(&mut self, rng: &mut StdRng, positions_per_user: usize) {
        let spacing = self.config.tick_spacing as i64;
        let state = self.pool_state();
        let seed_range = self.config.seed_amount_range;
        let min_tick = self.config.tick_range.0.div_euclid(spacing) * spacing;
        let max_tick = self.config.tick_range.1.div_euclid(spacing) * spacing;

        for user_idx in 0..self.users.len() {
            for _ in 0..positions_per_user {
                let (lower_tick, upper_tick) =
                    gen_in_range_ticks(rng, min_tick, max_tick, state.current_tick, spacing);

                // Integer math to avoid f64 precision loss on u128 values.
                // Safe: lower < current < upper for in-range positions, so diffs are positive.
                let total_range = (upper_tick - lower_tick) as u128;
                let base = rng.gen_range(seed_range.0..seed_range.1);
                let amount_0 = base * (upper_tick - state.current_tick) as u128 / total_range;
                let amount_1 = base * (state.current_tick - lower_tick) as u128 / total_range;

                let op = ConcentratedOp::AddLiquidity {
                    lower_tick,
                    upper_tick,
                    amount_0: amount_0.max(1),
                    amount_1: amount_1.max(1),
                    user_idx,
                };
                if let Err(e) = self.execute_op(&op) {
                    panic!(
                        "seed_positions: failed to add position for user {user_idx} \
                         at ticks [{lower_tick}, {upper_tick}]: {e}"
                    );
                }
            }
        }
    }

    /// Remove all tracked positions, switching to each position's owner.
    ///
    /// Full removals (fraction_bps=10000) auto-collect pending fees, so
    /// a single remove_liquidity call is sufficient to fully clean up
    /// each position — no separate collect_fees step needed.
    pub(crate) fn drain_all_liquidity(&mut self) {
        self.sync_positions();

        let mut i = self.positions.len();
        while i > 0 {
            i -= 1;
            let position_id = self.positions[i].position_id;
            let owner = self.position_owner(i);

            let on_chain_liquidity = self.query_position_liquidity(position_id);
            if on_chain_liquidity.is_zero() {
                self.positions.remove(i);
                continue;
            }

            let result = self.exec_as_sender(&owner, |pool| pool.remove_liquidity(i, 10_000));
            if let Err(e) = result {
                panic!(
                    "drain: remove failed for position {} (owner={}, liq={}): {}",
                    position_id, owner, on_chain_liquidity, e
                );
            }
        }

        if !self.positions.is_empty() {
            let remaining: Vec<String> = self
                .positions
                .iter()
                .map(|p| {
                    let liq = self.query_position_liquidity(p.position_id);
                    format!(
                        "  id={}, owner={}, liquidity={}",
                        p.position_id, p.owner, liq
                    )
                })
                .collect();
            panic!(
                "drain_all_liquidity: {} positions remain:\n{}",
                self.positions.len(),
                remaining.join("\n")
            );
        }
    }
}
