use std::time::Duration;

use crate::harness::concentrated::{ConcentratedConfig, ConcentratedPool};
use crate::runner::{fuzz_seed, FuzzPool, FuzzRunner};

fn endurance_config() -> ConcentratedConfig {
    ConcentratedConfig {
        num_users: 5,
        tick_range: (-10_000, 10_000),
        // Higher initial amounts to sustain more operations
        initial_amount_0: 500_000,
        initial_amount_1: 500_000,
        // Wider amount ranges for more diverse operations
        swap_amount_range: (50, 100_000),
        add_amount_range: (500, 200_000),
        seed_amount_range: (10_000, 100_000),
        // Slightly higher swap weight for more tick crossings and fee generation
        weight_swap: 45,
        weight_add: 25,
        weight_remove: 15,
        weight_collect: 15,
        ..ConcentratedConfig::default()
    }
}

/// Long-running endurance fuzz test: 5 users, wide tick range, all invariants
/// checked after every operation, with periodic progress reports.
///
/// Runs for 90 minutes by default. Use `--ignored` to include in test runs:
/// ```
/// cargo test -p tests-fuzz -- endurance --ignored --nocapture
/// ```
///
/// The test exercises the full lifecycle:
/// 1. Setup: 5 users, wide tick range, aggressive amounts
/// 2. Seed: 3 positions per user (15 total)
/// 3. Mixed fuzz: random ops for the full duration
/// 4. Drain: remove all positions
/// 5. Post-test: verify clean state (P1, P2)
#[test]
#[ignore]
fn test_endurance_1h() {
    let config = endurance_config();
    let mut runner = FuzzRunner::<ConcentratedPool>::new(&config, fuzz_seed());

    println!(
        "Seeding 3 positions per user ({} users)...",
        config.num_users
    );
    runner.seed(3);

    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_snapshot_invariants(&snapshot)
        .assert_all_pass();
    println!("Initial invariants OK. Starting endurance run.");

    runner.run_mixed_timed(
        Duration::from_secs(60 * 90), // 90 minutes
        Duration::from_secs(60),      // report every 60 seconds
    );

    println!("Draining all positions...");
    runner.pool.drain_all_liquidity();

    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
    println!("Post-test invariants OK. Endurance test passed.");
}

/// Shorter endurance test (10 minutes) for pre-merge validation.
///
/// ```
/// cargo test -p tests-fuzz -- endurance_10m --ignored --nocapture
/// ```
#[test]
#[ignore]
fn test_endurance_10m() {
    let config = endurance_config();
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);

    runner.seed(3);

    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_snapshot_invariants(&snapshot)
        .assert_all_pass();

    runner.run_mixed_timed(
        Duration::from_secs(10 * 60), // 10 minutes
        Duration::from_secs(30),      // report every 30 seconds
    );

    runner.pool.drain_all_liquidity();

    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}
