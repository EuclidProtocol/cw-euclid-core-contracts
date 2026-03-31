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
