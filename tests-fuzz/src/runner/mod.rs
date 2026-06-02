use std::time::{Duration, Instant, SystemTime};

use rand::rngs::StdRng;
use rand::SeedableRng;

mod coverage;
mod pool;
mod stats;

pub use coverage::InvariantCoverage;
pub use pool::FuzzPool;
pub use stats::RunStats;

use crate::invariants::InvariantResult;
use stats::humanize_duration;

/// Generic fuzz runner parameterized by pool type.
pub struct FuzzRunner<P: FuzzPool> {
    pub pool: P,
    pub rng: StdRng,
    pub seed: u64,
    pub stats: RunStats,
    pub coverage: InvariantCoverage,
}

/// Get a seed from `FUZZ_SEED` env var (for reproducibility) or generate
/// a random one from system entropy. Always prints the seed so failures
/// can be reproduced with `FUZZ_SEED=<value>`.
pub fn fuzz_seed() -> u64 {
    match std::env::var("FUZZ_SEED") {
        Ok(val) => {
            let seed = val.parse::<u64>().expect("FUZZ_SEED must be a u64");
            println!("Using FUZZ_SEED={}", seed);
            seed
        }
        Err(_) => {
            let seed = rand::random::<u64>();
            println!("Random seed={} (reproduce with FUZZ_SEED={})", seed, seed);
            seed
        }
    }
}

impl<P: FuzzPool> FuzzRunner<P> {
    /// Create a runner with a fresh pool from config and a deterministic RNG.
    pub fn new(config: &P::Config, seed: u64) -> Self {
        Self {
            pool: P::setup(config),
            rng: StdRng::seed_from_u64(seed),
            seed,
            stats: RunStats::new(),
            coverage: InvariantCoverage::new(),
        }
    }

    /// Create a runner with a random seed (or from FUZZ_SEED env var).
    pub fn new_random(config: &P::Config) -> Self {
        Self::new(config, fuzz_seed())
    }

    /// Seed initial liquidity positions.
    pub fn seed(&mut self, num_positions: usize) {
        self.pool.seed_liquidity(&mut self.rng, num_positions);
    }

    /// Execute an operation, timing it, and record the result in stats.
    fn timed_execute(&mut self, op: &P::Op) -> (Result<(), String>, Duration) {
        let start = Instant::now();
        let result = self.pool.execute_op(op);
        let elapsed = start.elapsed();
        let op_name = P::op_name(op);
        self.stats.record(op_name, &result, elapsed);
        (result, elapsed)
    }

    /// Assert that snapshot invariants pass, panicking with context on failure.
    fn assert_snapshot(&self, result: &InvariantResult, op_idx: u64, op: &P::Op) {
        if result.all_passed() {
            return;
        }
        let details: Vec<String> = result
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| format!("  {} — {}", c.name, c.detail))
            .collect();
        panic!(
            "Snapshot invariant violated at op {} ({:?}, seed={}):\n{}",
            op_idx,
            op,
            self.seed,
            details.join("\n")
        );
    }

    /// Assert that transition invariants pass, panicking with context on failure.
    fn assert_transition(&self, result: &InvariantResult, op_idx: u64, op: &P::Op) {
        if result.all_passed() {
            return;
        }
        let details: Vec<String> = result
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| format!("  {} — {}", c.name, c.detail))
            .collect();
        panic!(
            "Transition invariant violated at op {} ({:?}, seed={}):\n{}",
            op_idx,
            op,
            self.seed,
            details.join("\n")
        );
    }

    /// Execute one op with invariant checks, returning the post-op snapshot.
    ///
    /// Core loop body shared by [`run_mixed`] and [`run_mixed_timed`]:
    /// 1. Execute the op (timed)
    /// 2. On success: take post-op snapshot, check transition invariants
    /// 3. On failure: log (if `log_failure`), reuse or refresh snapshot
    /// 4. Check snapshot invariants on the resulting state
    ///
    /// When `full_check` is true, uses full snapshots and all invariants.
    /// When false, uses light snapshots and cheap invariants.
    fn execute_checked(
        &mut self,
        op: &P::Op,
        before: P::Snapshot,
        op_idx: u64,
        full_check: bool,
        log_failure: bool,
    ) -> P::Snapshot {
        let (result, _elapsed) = self.timed_execute(op);

        let current = match result {
            Ok(()) => {
                let after = if full_check {
                    self.pool.snapshot()
                } else {
                    self.pool.light_snapshot()
                };
                let transition = self.pool.check_transition_invariants(&before, &after, op);
                self.assert_transition(&transition, op_idx, op);
                self.coverage.record(&transition);
                after
            }
            Err(e) => {
                if log_failure {
                    println!(
                        "  [op {} FAILED] {} — {:?}: {}",
                        op_idx,
                        P::op_name(op),
                        op,
                        e
                    );
                }
                // State unchanged on error. If this is a full-check cycle,
                // take a fresh full snapshot (before may be light).
                // Otherwise reuse before to avoid unnecessary queries.
                if full_check {
                    self.pool.snapshot()
                } else {
                    before
                }
            }
        };

        let snapshot_result = if full_check {
            self.pool.check_snapshot_invariants(&current)
        } else {
            self.pool.check_light_snapshot_invariants(&current)
        };
        self.assert_snapshot(&snapshot_result, op_idx, op);
        self.coverage.record(&snapshot_result);

        current
    }

    /// Run random operations with full snapshot + transition invariant checks after every op.
    pub fn run_mixed(&mut self, num_ops: u64) {
        println!(
            "Starting mixed run: {} ops on {}, seed={}",
            num_ops,
            self.pool.pool_name(),
            self.seed,
        );

        for i in 0..num_ops {
            let op = self.pool.random_op(&mut self.rng);
            let before = self.pool.snapshot();
            self.execute_checked(&op, before, i, true, true);
        }

        self.stats.print_summary(&self.pool.pool_name(), self.seed);
        self.coverage.print_report();
    }

    /// Phased run: seed positions → random ops → drain all → assert clean state.
    pub fn run_linear(&mut self, num_positions: usize, num_ops: u64) {
        println!("Phase 1: Seeding {} positions...", num_positions);
        self.seed(num_positions);
        println!("  Created positions");

        let snapshot = self.pool.snapshot();
        let result = self.pool.check_snapshot_invariants(&snapshot);
        self.coverage.record(&result);
        result.assert_all_pass();

        println!("Phase 2: Executing {} ops...", num_ops);
        for i in 0..num_ops {
            let op = self.pool.random_op(&mut self.rng);
            let before = self.pool.snapshot();
            self.execute_checked(&op, before, i, true, true);
        }

        println!("Phase 3: Removing all positions...");
        self.pool.drain_all_liquidity();

        println!("Phase 4: Asserting clean state...");
        let snapshot = self.pool.snapshot();
        let result = self.pool.check_post_test_invariants(&snapshot);
        self.coverage.record(&result);
        result.assert_all_pass();

        println!("Linear fuzz complete (seed={})", self.seed);
        self.coverage.print_report();
    }

    /// Run random operations on a single pool until wall-clock time expires.
    ///
    /// Uses two-tier invariant checking to maintain throughput at scale:
    /// - **Every op**: light snapshot + cheap invariants
    /// - **Every Nth op**: full snapshot + all invariants
    ///
    /// Caches snapshots across iterations to avoid redundant queries.
    /// Prints progress every `report_interval` seconds.
    pub fn run_mixed_timed(&mut self, duration: Duration, report_interval: Duration) {
        const FULL_CHECK_INTERVAL: u64 = 20;

        let start = SystemTime::now();
        let end = start + duration;
        let mut next_report = start + report_interval;
        let mut op_idx: u64 = 0;

        println!(
            "Starting timed run: {} on {}, seed={} (full check every {} ops)",
            humanize_duration(duration),
            self.pool.pool_name(),
            self.seed,
            FULL_CHECK_INTERVAL,
        );

        // Take initial full snapshot so the first full-check cycle (op 0)
        // gets a coherent before/after pair. After each op, `current` becomes
        // the next `before`, cutting snapshot queries from 2 per op to 1.
        let mut current = self.pool.snapshot();

        while SystemTime::now() < end {
            let op = self.pool.random_op(&mut self.rng);
            let is_full_check = op_idx % FULL_CHECK_INTERVAL == 0;

            current = self.execute_checked(&op, current, op_idx, is_full_check, op_idx < 50);

            op_idx += 1;

            if SystemTime::now() >= next_report {
                let elapsed = SystemTime::now().duration_since(start).unwrap_or_default();
                let remaining = duration.saturating_sub(elapsed);
                let err_info = self.stats.error_breakdown();
                let err_line = if err_info.is_empty() {
                    String::new()
                } else {
                    format!("\n    {}", err_info)
                };
                println!(
                    "  [{}] {} ops ({} ok, {} err), ~{:.0} ops/sec, {} remaining\n    {}{}",
                    humanize_duration(elapsed),
                    self.stats.total_ops,
                    self.stats.success_count,
                    self.stats.error_count,
                    self.stats.total_ops as f64 / elapsed.as_secs_f64(),
                    humanize_duration(remaining),
                    self.stats.op_breakdown(),
                    err_line,
                );
                next_report = SystemTime::now() + report_interval;
            }
        }

        let elapsed = SystemTime::now().duration_since(start).unwrap_or_default();
        println!(
            "Timed run complete: {} ops in {} ({:.1} ops/sec)",
            self.stats.total_ops,
            humanize_duration(elapsed),
            self.stats.total_ops as f64 / elapsed.as_secs_f64(),
        );
        self.stats.print_summary(&self.pool.pool_name(), self.seed);
        self.coverage.print_report();
    }

    /// Run fresh pools repeatedly until wall-clock time expires.
    /// Each iteration uses a unique seed for reproducibility.
    pub fn run_for_duration(
        config: &P::Config,
        duration_secs: u64,
        ops_per_iter: u64,
        seed: u64,
        seed_positions: usize,
    ) {
        let start = SystemTime::now();
        let end = start + Duration::from_secs(duration_secs);
        let mut total_ops = 0u64;
        let mut iterations = 0u64;
        let mut aggregate_coverage = InvariantCoverage::new();

        println!(
            "Starting duration run: {} with {} ops/iter, {} positions/iter, base seed={}",
            humanize_duration(Duration::from_secs(duration_secs)),
            ops_per_iter,
            seed_positions,
            seed,
        );

        let mut aggregate_stats = RunStats::new();

        while SystemTime::now() <= end {
            let iter_seed = seed.wrapping_add(iterations);
            let mut runner = FuzzRunner::<P>::new(config, iter_seed);
            runner.seed(seed_positions);
            runner.run_mixed(ops_per_iter);
            aggregate_coverage.merge(&runner.coverage);
            aggregate_stats.merge(&runner.stats);
            total_ops += ops_per_iter;
            iterations += 1;
        }

        let elapsed = SystemTime::now().duration_since(start).unwrap_or_default();
        println!(
            "Duration run complete: {} ops in {} iterations ({}, {:.1} ops/sec)",
            total_ops,
            iterations,
            humanize_duration(elapsed),
            total_ops as f64 / elapsed.as_secs_f64(),
        );
        aggregate_stats.print_summary("aggregate", seed);
        aggregate_coverage.print_report();
    }
}
