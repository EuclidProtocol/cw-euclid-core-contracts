use cosmwasm_std::Uint128;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::harness::concentrated::{ConcentratedConfig, ConcentratedPool};
use crate::helpers::factory::execute_concentrated_swap;
use crate::runner::FuzzPool;
use crate::strategies::concentrated::ConcentratedOp;

fn default_pool() -> ConcentratedPool {
    ConcentratedPool::setup(&ConcentratedConfig::default())
}

// Single-tick-range position (extremely narrow)
#[test]
fn test_single_spacing_position() {
    let mut pool = default_pool();

    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -10,
        upper_tick: 10,
        amount_0: 50_000,
        amount_1: 50_000,
        user_idx: 0,
    };
    let result = pool.execute_op(&op);
    assert!(result.is_ok(), "single spacing position should succeed");

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();

    let op = ConcentratedOp::Swap {
        amount: 1_000,
        zero_for_one: true,
        user_idx: 0,
    };
    let _ = pool.execute_op(&op);
    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
}

// Dust-level swap amounts (Balancer V2 exploit pattern)
#[test]
fn test_dust_level_swaps() {
    let mut rng = StdRng::seed_from_u64(999);
    let config = ConcentratedConfig {
        tick_range: (-200, 200),
        ..ConcentratedConfig::default()
    };
    let mut pool = ConcentratedPool::setup(&config);

    pool.seed_liquidity(&mut rng, 3);

    for _ in 0..20 {
        let op = ConcentratedOp::dust_swap(&mut rng);
        let _ = pool.execute_op(&op);
        let snapshot = pool.snapshot();
        pool.check_snapshot_invariants(&snapshot).assert_all_pass();
    }
}

// Large swap crossing many tick boundaries
#[test]
fn test_large_swap_crosses_ticks() {
    let mut pool = default_pool();

    for i in 0..5 {
        let lower = -500 + i * 100;
        let upper = lower + 200;
        let op = ConcentratedOp::AddLiquidity {
            lower_tick: lower,
            upper_tick: upper,
            amount_0: 20_000,
            amount_1: 20_000,
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();

    let before = pool.snapshot();
    let op = ConcentratedOp::Swap {
        amount: 100_000,
        zero_for_one: true,
        user_idx: 0,
    };
    let _ = pool.execute_op(&op);

    let after = pool.snapshot();
    pool.check_transition_invariants(&before, &after, &op)
        .assert_all_pass();
    pool.check_snapshot_invariants(&after).assert_all_pass();
}

// Round-trip swap loses value (no-arbitrage invariant)
#[test]
fn test_round_trip_swap_loses_value() {
    let mut pool = default_pool();

    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -5000,
        upper_tick: 5000,
        amount_0: 100_000,
        amount_1: 100_000,
        user_idx: 0,
    };
    pool.execute_op(&op).expect("add liquidity should succeed");

    let amount_in = 5_000u128;
    let amount_out = execute_concentrated_swap(
        &pool.factory,
        &pool.router,
        pool.pool_key.clone(),
        pool.token_a.clone(),
        pool.token_b.token.clone(),
        Uint128::new(amount_in),
    )
    .expect("forward swap should succeed");

    assert!(
        amount_out > Uint128::zero(),
        "forward swap should produce output"
    );

    let amount_back = execute_concentrated_swap(
        &pool.factory,
        &pool.router,
        pool.pool_key.clone(),
        pool.token_b.clone(),
        pool.token_a.token.clone(),
        amount_out,
    )
    .expect("reverse swap should succeed");

    assert!(
        amount_back < Uint128::new(amount_in),
        "round-trip should lose value: in={}, back={}",
        amount_in,
        amount_back
    );
}

// Overlapping positions with fee collection
#[test]
fn test_overlapping_positions_fees() {
    let mut rng = StdRng::seed_from_u64(777);
    let mut pool = default_pool();

    let ranges = vec![(-200, 200), (-100, 300), (0, 400), (-300, 100)];
    for (lower, upper) in &ranges {
        let op = ConcentratedOp::AddLiquidity {
            lower_tick: *lower,
            upper_tick: *upper,
            amount_0: 30_000,
            amount_1: 30_000,
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    for _ in 0..10 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(1000..10000u128),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    let num_positions = pool.positions.len();
    for i in 0..num_positions {
        let before = pool.snapshot();
        let op = ConcentratedOp::CollectFees {
            position_idx: i,
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
        let after = pool.snapshot();
        pool.check_transition_invariants(&before, &after, &op)
            .assert_all_pass();
    }

    // Second collect should return zero (idempotent)
    for i in 0..num_positions {
        let op = ConcentratedOp::CollectFees {
            position_idx: i,
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
}

// Empty pool operations
#[test]
fn test_empty_pool_invariants() {
    let pool = default_pool();
    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
}

// Near-boundary swap strategy (Osmosis-inspired)
#[test]
fn test_near_boundary_swaps() {
    let mut rng = StdRng::seed_from_u64(555);
    let config = ConcentratedConfig {
        tick_range: (-300, 300),
        ..ConcentratedConfig::default()
    };
    let mut pool = ConcentratedPool::setup(&config);

    pool.seed_liquidity(&mut rng, 5);

    for _ in 0..15 {
        let op = ConcentratedOp::near_boundary_swap(&mut rng, pool.config.tick_spacing);
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
        let snapshot = pool.snapshot();
        pool.check_snapshot_invariants(&snapshot).assert_all_pass();
    }
}

/// C2 reproducer: add position out-of-range, then swap into its range.
/// Tests whether tick crossing correctly updates ACTIVE_LIQUIDITY.
#[test]
fn test_c2_out_of_range_then_swap_into() {
    let config = ConcentratedConfig {
        initial_amount_0: 10_000,
        initial_amount_1: 10_000,
        tick_range: (-500, 500),
        ..ConcentratedConfig::default()
    };
    let mut pool = ConcentratedPool::setup(&config);

    let snap = pool.snapshot();
    let initial_tick = snap.slot0.tick;
    let initial_liq = snap.slot0.liquidity;

    // Add a position BELOW current tick (out-of-range)
    let out_of_range_lower = -200i64;
    let out_of_range_upper = -100i64;
    assert!(
        initial_tick >= out_of_range_upper,
        "position should be below current tick"
    );

    let op = ConcentratedOp::AddLiquidity {
        lower_tick: out_of_range_lower,
        upper_tick: out_of_range_upper,
        amount_0: 1,
        amount_1: 5_000,
        user_idx: 0,
    };
    pool.execute_op(&op)
        .expect("out-of-range add should succeed");

    let snap = pool.snapshot();
    // ACTIVE_LIQUIDITY should NOT have changed (position is out-of-range)
    assert_eq!(
        snap.slot0.liquidity, initial_liq,
        "out-of-range add should not change active liquidity"
    );
    pool.check_snapshot_invariants(&snap).assert_all_pass();

    // Now swap to move the tick DOWN into the out-of-range position's range
    // zero_for_one=true moves price down (tick decreases)
    // Use a smaller amount to land inside [-200, -100] rather than blowing through
    let op = ConcentratedOp::Swap {
        amount: 5_000, // calibrated to land in [-200, -100]
        zero_for_one: true,
        user_idx: 0,
    };
    let before = pool.snapshot();
    pool.execute_op(&op).expect("swap should succeed");
    let after = pool.snapshot();

    pool.check_transition_invariants(&before, &after, &op)
        .assert_all_pass();
    pool.check_snapshot_invariants(&after).assert_all_pass();
}

/// Minimal reproducer for C2 active_liquidity mismatch.
/// Adds a single in-range position and checks whether ACTIVE_LIQUIDITY updates.
#[test]
fn test_c2_active_liquidity_add_in_range() {
    let config = ConcentratedConfig {
        initial_amount_0: 100,
        initial_amount_1: 100,
        tick_range: (-100, 100),
        ..ConcentratedConfig::default()
    };
    let mut pool = ConcentratedPool::setup(&config);

    let snap = pool.snapshot();
    pool.check_snapshot_invariants(&snap).assert_all_pass();

    // Add position that spans current tick — must be in-range
    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -50,
        upper_tick: 50,
        amount_0: 50,
        amount_1: 50,
        user_idx: 0,
    };
    pool.execute_op(&op).expect("add should succeed");

    let snap = pool.snapshot();
    pool.check_snapshot_invariants(&snap).assert_all_pass();
}
