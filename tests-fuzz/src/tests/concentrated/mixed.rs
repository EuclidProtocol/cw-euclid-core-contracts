use crate::harness::concentrated::{ConcentratedConfig, ConcentratedPool};
use crate::runner::{fuzz_seed, FuzzPool, FuzzRunner};

/// Narrow tick range (-100, 100), 50 random ops with full invariant checks.
#[test]
fn test_concentrated_fuzz_mixed_small_range() {
    let config = ConcentratedConfig {
        tick_range: (-100, 100),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(3);
    runner.run_mixed(50);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

/// Medium tick range (-1000, 1000), 30 random ops with full invariant checks.
#[test]
fn test_concentrated_fuzz_mixed_medium_range() {
    let config = ConcentratedConfig {
        tick_range: (-1000, 1000),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(3);
    runner.run_mixed(30);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

/// Wide tick range (-10000, 10000), 500 random ops with full invariant checks.
#[test]
fn test_concentrated_fuzz_mixed_wide_range() {
    let config = ConcentratedConfig {
        tick_range: (-10000, 10000),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(3);
    runner.run_mixed(500);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

/// Timed run: 10 seconds, fresh pool each iteration with incrementing seeds.
#[test]
fn test_concentrated_fuzz_mixed_timed() {
    let config = ConcentratedConfig::default();
    FuzzRunner::<ConcentratedPool>::run_for_duration(&config, 10, 2000, fuzz_seed(), 3);
}

/// High fee tier (1%, tick_spacing=200). Catches fee accumulation bugs at
/// aggressive fee rates.
#[test]
fn test_concentrated_fuzz_mixed_high_fee() {
    let config = ConcentratedConfig {
        fee_tier_bps: 10_000,
        tick_spacing: 200,
        num_users: 3,
        tick_range: (-10_000, 10_000),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(3);
    runner.run_mixed(30);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

/// Tiny amounts — catches rounding/dust bugs at small scales.
#[test]
fn test_concentrated_fuzz_mixed_tiny_amounts() {
    let config = ConcentratedConfig {
        num_users: 3,
        initial_amount_0: 500,
        initial_amount_1: 500,
        swap_amount_range: (10, 200),
        add_amount_range: (50, 500),
        seed_amount_range: (50, 200),
        tick_range: (-200, 200),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(3);
    runner.run_mixed(30);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

/// Wide tick spacing (60, fee_tier=3000). Tests coarser granularity pools.
#[test]
fn test_concentrated_fuzz_mixed_wide_spacing() {
    let config = ConcentratedConfig {
        fee_tier_bps: 3_000,
        tick_spacing: 60,
        num_users: 3,
        tick_range: (-6_000, 6_000),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(3);
    runner.run_mixed(30);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

/// Large values (10^12 tokens) — stresses Uint256 intermediate math.
#[test]
fn test_concentrated_fuzz_mixed_large_values() {
    let config = ConcentratedConfig {
        num_users: 3,
        initial_amount_0: 1_000_000_000_000,
        initial_amount_1: 1_000_000_000_000,
        swap_amount_range: (1_000_000, 500_000_000_000),
        add_amount_range: (1_000_000_000, 500_000_000_000),
        seed_amount_range: (1_000_000_000, 100_000_000_000),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(3);
    runner.run_mixed(30);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}
