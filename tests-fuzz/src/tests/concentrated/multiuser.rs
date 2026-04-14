use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::harness::concentrated::{ConcentratedConfig, ConcentratedPool};
use crate::runner::{fuzz_seed, FuzzPool, FuzzRunner};
use crate::strategies::concentrated::ConcentratedOp;

fn multiuser_config(num_users: usize) -> ConcentratedConfig {
    ConcentratedConfig {
        num_users,
        ..ConcentratedConfig::default()
    }
}

// =============================================================================
// Multi-user mixed fuzz (via FuzzRunner)
// =============================================================================

/// 3 users, 60 random operations, invariant checks after every op, then drain.
#[test]
fn test_multiuser_fuzz_mixed() {
    let config = multiuser_config(3);
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(2);
    runner.run_mixed(60);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

// Multiple users seed positions, swap, then drain — full lifecycle.
#[test]
fn test_multiuser_linear_lifecycle() {
    let config = multiuser_config(3);
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.run_linear(3, 30);
}

// 4 users adding/removing liquidity concurrently, heavier load.
#[test]
fn test_multiuser_concurrent_liquidity() {
    let config = multiuser_config(4);
    let mut runner = FuzzRunner::<ConcentratedPool>::new_random(&config);
    runner.seed(2);
    runner.run_mixed(40);

    runner.pool.drain_all_liquidity();
    let snapshot = runner.pool.snapshot();
    runner
        .pool
        .check_post_test_invariants(&snapshot)
        .assert_all_pass();
}

// =============================================================================
// Targeted multi-user scenarios
// =============================================================================

// Sandwich attack pattern: user A provides liquidity, user B front-runs with a
// swap, user A's position value is affected.
#[test]
fn test_sandwich_attack_pattern() {
    let config = multiuser_config(3);
    let mut pool = ConcentratedPool::setup(&config);

    // User 0 (victim) provides wide liquidity
    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -500,
        upper_tick: 500,
        amount_0: 50_000,
        amount_1: 50_000,
        user_idx: 0,
    };
    pool.execute_op(&op)
        .expect("victim liquidity should succeed");

    // User 1 (attacker) front-runs: large swap pushing price
    let front_run_op = ConcentratedOp::Swap {
        amount: 30_000,
        zero_for_one: true,
        user_idx: 1,
    };
    let before = pool.snapshot();
    pool.execute_op(&front_run_op)
        .expect("front-run swap should succeed");
    let after = pool.snapshot();
    pool.check_transition_invariants(&before, &after, &front_run_op)
        .assert_all_pass();
    pool.check_snapshot_invariants(&after).assert_all_pass();

    // User 2 (victim tx) swaps at the moved price
    let victim_op = ConcentratedOp::Swap {
        amount: 5_000,
        zero_for_one: true,
        user_idx: 2,
    };
    let before = pool.snapshot();
    pool.execute_op(&victim_op)
        .expect("victim swap should succeed");
    let after = pool.snapshot();
    pool.check_transition_invariants(&before, &after, &victim_op)
        .assert_all_pass();
    pool.check_snapshot_invariants(&after).assert_all_pass();

    // User 1 (attacker) back-runs: reverse swap
    let backrun_op = ConcentratedOp::Swap {
        amount: 25_000,
        zero_for_one: false,
        user_idx: 1,
    };
    let before = pool.snapshot();
    pool.execute_op(&backrun_op)
        .expect("back-run swap should succeed");
    let after = pool.snapshot();
    pool.check_transition_invariants(&before, &after, &backrun_op)
        .assert_all_pass();
    pool.check_snapshot_invariants(&after).assert_all_pass();
}

// Cross-user fee isolation: users have overlapping positions, fees should
// accrue independently based on each position's liquidity share.
#[test]
fn test_cross_user_fee_isolation() {
    let mut rng = StdRng::seed_from_u64(fuzz_seed());
    let config = multiuser_config(2);
    let mut pool = ConcentratedPool::setup(&config);

    // User 0: wide position
    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -500,
        upper_tick: 500,
        amount_0: 50_000,
        amount_1: 50_000,
        user_idx: 0,
    };
    pool.execute_op(&op).expect("user 0 add should succeed");

    // User 1: narrow overlapping position at current tick
    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -100,
        upper_tick: 100,
        amount_0: 50_000,
        amount_1: 50_000,
        user_idx: 1,
    };
    pool.execute_op(&op).expect("user 1 add should succeed");

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();

    // Generate fees via swaps from user 0
    for _ in 0..10 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(1000..10000u128),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    // Both users collect fees — each from their own positions (user-relative indices)
    for user_idx in 0..2 {
        let num_user_positions = pool.positions_for_user(user_idx).len();
        for relative_idx in 0..num_user_positions {
            let before = pool.snapshot();
            let op = ConcentratedOp::CollectFees {
                position_idx: relative_idx,
                user_idx,
            };
            let _ = pool.execute_op(&op);
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
}

// JIT (Just-In-Time) liquidity pattern: user 1 adds concentrated liquidity
// right before a swap, capturing most fees, then removes immediately after.
#[test]
fn test_jit_liquidity_pattern() {
    let config = multiuser_config(2);
    let mut pool = ConcentratedPool::setup(&config);

    // User 0: provides base liquidity
    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -500,
        upper_tick: 500,
        amount_0: 30_000,
        amount_1: 30_000,
        user_idx: 0,
    };
    pool.execute_op(&op).expect("base liquidity should succeed");

    // User 1: JIT — adds very concentrated liquidity right at current tick
    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -10,
        upper_tick: 10,
        amount_0: 80_000,
        amount_1: 80_000,
        user_idx: 1,
    };
    pool.execute_op(&op).expect("JIT liquidity should succeed");

    let before_swap = pool.snapshot();

    // A swap occurs — user 1's concentrated position captures most fees
    let op = ConcentratedOp::Swap {
        amount: 10_000,
        zero_for_one: true,
        user_idx: 0,
    };
    pool.execute_op(&op).expect("JIT swap should succeed");

    let after_swap = pool.snapshot();
    pool.check_transition_invariants(&before_swap, &after_swap, &op)
        .assert_all_pass();
    pool.check_snapshot_invariants(&after_swap)
        .assert_all_pass();

    // User 1: immediately removes JIT position
    let user1_positions = pool.positions_for_user(1);
    if !user1_positions.is_empty() {
        let op = ConcentratedOp::RemoveLiquidity {
            position_idx: 0,
            fraction_bps: 10_000,
            user_idx: 1,
        };
        let before = pool.snapshot();
        let _ = pool.execute_op(&op);
        let after = pool.snapshot();
        pool.check_transition_invariants(&before, &after, &op)
            .assert_all_pass();
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
}

// =============================================================================
// Authorization tests
// =============================================================================

// User B should not be able to remove liquidity from user A's position.
#[test]
fn test_unauthorized_remove_fails() {
    let config = multiuser_config(2);
    let mut pool = ConcentratedPool::setup(&config);
    let initial_positions = pool.positions.len();

    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -200,
        upper_tick: 200,
        amount_0: 50_000,
        amount_1: 50_000,
        user_idx: 0,
    };
    pool.execute_op(&op).expect("user 0 add should succeed");

    assert_eq!(pool.positions.len(), initial_positions + 1);
    let position_id = pool
        .positions
        .last()
        .expect("should have at least one position after add")
        .position_id;

    // Try to remove user 0's position as user 1
    let pool_key = pool.pool_key.clone();
    let result = pool.exec_as_user(1, |p| {
        crate::helpers::factory::remove_concentrated_liquidity(
            &p.factory,
            &p.router,
            pool_key,
            position_id,
            cosmwasm_std::Uint128::new(1),
        )
    });

    assert!(
        result.is_err(),
        "user 1 should NOT be able to remove user 0's position, but got Ok"
    );

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
}

// User B should not be able to collect fees from user A's position.
#[test]
fn test_unauthorized_collect_fails() {
    let mut rng = StdRng::seed_from_u64(fuzz_seed());
    let config = multiuser_config(2);
    let mut pool = ConcentratedPool::setup(&config);

    let op = ConcentratedOp::AddLiquidity {
        lower_tick: -200,
        upper_tick: 200,
        amount_0: 50_000,
        amount_1: 50_000,
        user_idx: 0,
    };
    pool.execute_op(&op).expect("user 0 add should succeed");

    for _ in 0..5 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(1000..10000u128),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    let position_id = pool
        .positions
        .last()
        .expect("should have at least one position after add")
        .position_id;
    let pool_key = pool.pool_key.clone();

    // Try to collect fees from user 0's position as user 1
    let result = pool.exec_as_user(1, |p| {
        let chain_uid = p.chain_uid();
        let fake_recipient =
            euclid::cross_chain_user::CrossChainUser::new(chain_uid, p.user_addr(1).to_string());
        crate::helpers::factory::collect_concentrated_fees(
            &p.factory,
            &p.router,
            pool_key,
            position_id,
            fake_recipient,
        )
    });

    assert!(
        result.is_err(),
        "user 1 should NOT be able to collect fees from user 0's position, but got Ok"
    );

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
}
