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

/// Multi-config endurance: runs several pool configurations sequentially,
/// each with a different seed. Catches bugs that only manifest with specific
/// fee tiers, tick spacings, or amount scales.
///
/// ```
/// cargo test -p tests-fuzz -- endurance_multi_config --ignored --nocapture
/// ```
#[test]
#[ignore]
fn test_endurance_multi_config() {
    let configs: Vec<(&str, ConcentratedConfig, u64)> = vec![
        (
            "high_fee",
            ConcentratedConfig {
                fee_tier_bps: 10_000, // 1% fee
                tick_spacing: 200,    // required for fee_tier=10000
                num_users: 3,
                tick_range: (-10_000, 10_000),
                initial_amount_0: 200_000,
                initial_amount_1: 200_000,
                ..ConcentratedConfig::default()
            },
            0xCAFE_0001,
        ),
        (
            "tiny_amounts",
            ConcentratedConfig {
                num_users: 3,
                initial_amount_0: 500,
                initial_amount_1: 500,
                swap_amount_range: (10, 200),
                add_amount_range: (50, 500),
                seed_amount_range: (50, 200),
                tick_range: (-200, 200),
                ..ConcentratedConfig::default()
            },
            0xCAFE_0002,
        ),
        (
            "wide_spacing",
            ConcentratedConfig {
                fee_tier_bps: 3_000, // 0.3% fee
                tick_spacing: 60,    // required for fee_tier=3000
                num_users: 3,
                tick_range: (-6_000, 6_000),
                initial_amount_0: 500_000,
                initial_amount_1: 500_000,
                ..ConcentratedConfig::default()
            },
            0xCAFE_0003,
        ),
        (
            "large_values",
            ConcentratedConfig {
                num_users: 3,
                initial_amount_0: 1_000_000_000_000,
                initial_amount_1: 1_000_000_000_000,
                swap_amount_range: (1_000_000, 500_000_000_000),
                add_amount_range: (1_000_000_000, 500_000_000_000),
                seed_amount_range: (1_000_000_000, 100_000_000_000),
                ..ConcentratedConfig::default()
            },
            0xCAFE_0004,
        ),
    ];

    for (name, config, seed) in configs {
        println!("\n=== Config: {} (seed={:#X}) ===", name, seed);
        let mut runner = FuzzRunner::<ConcentratedPool>::new(&config, seed);

        runner.seed(3);
        let snapshot = runner.pool.snapshot();
        runner
            .pool
            .check_snapshot_invariants(&snapshot)
            .assert_all_pass();

        runner.run_mixed_timed(
            Duration::from_secs(2 * 60), // 2 minutes per config
            Duration::from_secs(30),
        );

        runner.pool.drain_all_liquidity();
        let snapshot = runner.pool.snapshot();
        runner
            .pool
            .check_post_test_invariants(&snapshot)
            .assert_all_pass();
        println!("=== {} PASSED ===", name);
    }
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
