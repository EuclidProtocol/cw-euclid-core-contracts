use std::fmt::Debug;

use rand::rngs::StdRng;

use crate::invariants::InvariantResult;

/// Pool-specific fuzz harness. Each pool type (concentrated, CP, stable)
/// implements this to plug into the generic [`super::FuzzRunner`].
pub trait FuzzPool: Sized {
    type Op: Debug + Clone;
    type Snapshot: Debug + Clone;
    type Config: Debug + Clone;

    /// Deploy contracts and create a pool from config.
    fn setup(config: &Self::Config) -> Self;
    /// Generate a random operation using live pool state for smarter targeting.
    fn random_op(&self, rng: &mut StdRng) -> Self::Op;
    /// Execute an operation. Returns Err for expected failures (e.g. no liquidity).
    fn execute_op(&mut self, op: &Self::Op) -> Result<(), String>;
    /// Capture full pool state for invariant checking.
    fn snapshot(&self) -> Self::Snapshot;
    /// Capture lightweight pool state for cheap per-op checks.
    /// Default: falls back to full snapshot. Override for O(1) per-op checks.
    fn light_snapshot(&self) -> Self::Snapshot {
        self.snapshot()
    }
    /// Check snapshot invariants (must hold at any point in time).
    fn check_snapshot_invariants(&self, snapshot: &Self::Snapshot) -> InvariantResult;
    /// Check cheap invariants using lightweight state data.
    /// Default: falls back to full check. Override for O(1) per-op checks.
    fn check_light_snapshot_invariants(&self, snapshot: &Self::Snapshot) -> InvariantResult {
        self.check_snapshot_invariants(snapshot)
    }
    /// Check transition invariants (compare before/after a successful operation).
    fn check_transition_invariants(
        &self,
        before: &Self::Snapshot,
        after: &Self::Snapshot,
        op: &Self::Op,
    ) -> InvariantResult;
    /// Check post-test invariants (must hold after all positions are drained).
    fn check_post_test_invariants(&self, snapshot: &Self::Snapshot) -> InvariantResult;
    /// Add random in-range liquidity positions as starting state.
    fn seed_liquidity(&mut self, rng: &mut StdRng, num_positions: usize);
    /// Remove all tracked positions. Panics if removal gets stuck.
    fn drain_all_liquidity(&mut self);
    /// Human-readable name for an operation (for stats/logging).
    fn op_name(op: &Self::Op) -> &'static str;
    /// Human-readable pool description (for stats/logging).
    fn pool_name(&self) -> String;
}
