use crate::harness::concentrated::{ConcentratedConfig, ConcentratedPool};
use crate::runner::FuzzRunner;

/// Seed 3 positions, 20 random ops, drain, post-test assertions.
#[test]
fn test_concentrated_linear_small() {
    let config = ConcentratedConfig {
        tick_range: (-200, 200),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.run_linear(3, 20);
}

/// Seed 5 positions, 30 random ops, drain, post-test assertions.
#[test]
fn test_concentrated_linear_medium() {
    let config = ConcentratedConfig {
        tick_range: (-1000, 1000),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.run_linear(5, 30);
}

/// Seed 10 positions, 20 random ops, drain, post-test assertions.
#[test]
fn test_concentrated_linear_many_positions() {
    let config = ConcentratedConfig {
        tick_range: (-500, 500),
        ..ConcentratedConfig::default()
    };
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.run_linear(10, 20);
}
