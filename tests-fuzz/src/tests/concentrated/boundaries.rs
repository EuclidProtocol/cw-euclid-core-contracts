use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::harness::concentrated::{ConcentratedConfig, ConcentratedPool};
use crate::runner::FuzzPool;
use crate::strategies::concentrated::ConcentratedOp;

/// Config that maximizes fee accumulation rate:
/// - Minimum seed liquidity (tiny positions → huge fee_growth_delta per swap)
/// - High fee tier (500 bps = 5%)
/// - Large swap amounts relative to liquidity
fn high_fee_growth_config() -> ConcentratedConfig {
    ConcentratedConfig {
        fee_tier_bps: 500,
        tick_spacing: 10,
        slippage_tolerance_bps: 10_000,
        initial_amount_0: 1_000,
        initial_amount_1: 1_000,
        swap_amount_range: (100, 500),
        add_amount_range: (100, 1_000),
        seed_amount_range: (100, 500),
        tick_range: (-500, 500),
        ..ConcentratedConfig::default()
    }
}

/// Config with very large token amounts to push intermediate math
/// toward Uint128/Uint256 boundaries.
/// - Large reserves → large swap amounts → large fee amounts
/// - fee * 2^128 approaches Uint256 upper range
/// - liquidity * fee_growth_delta / 2^128 tests large multiply in fees_owed
fn large_value_config() -> ConcentratedConfig {
    ConcentratedConfig {
        fee_tier_bps: 500,
        tick_spacing: 10,
        slippage_tolerance_bps: 10_000,
        // Use values near u64::MAX to stress math without exceeding Uint128
        initial_amount_0: 1_000_000_000_000_000_000, // 10^18
        initial_amount_1: 1_000_000_000_000_000_000,
        swap_amount_range: (1_000_000_000_000_000, 500_000_000_000_000_000), // 10^15 to 5*10^17
        add_amount_range: (1_000_000_000_000_000, 500_000_000_000_000_000),
        seed_amount_range: (1_000_000_000_000_000, 100_000_000_000_000_000),
        tick_range: (-5_000, 5_000),
        ..ConcentratedConfig::default()
    }
}

/// Config with extreme tick boundaries.
fn extreme_tick_config() -> ConcentratedConfig {
    ConcentratedConfig {
        fee_tier_bps: 500,
        tick_spacing: 10,
        slippage_tolerance_bps: 10_000,
        initial_amount_0: 50_000,
        initial_amount_1: 50_000,
        tick_range: (-88_720, 88_720), // near MIN_TICK/MAX_TICK (-887272, 887272) / 10
        swap_amount_range: (1_000, 50_000),
        add_amount_range: (5_000, 50_000),
        seed_amount_range: (5_000, 50_000),
        ..ConcentratedConfig::default()
    }
}

// =============================================================================
// High fee_growth accumulation — push accumulators as high as possible
// =============================================================================

/// Hammer a pool with minimal liquidity and many swaps to maximize
/// fee_growth_global. Then add/remove/collect positions to exercise
/// fee_growth_inside and fees_owed with large accumulator values.
///
/// fee_growth_delta = lp_fee * 2^128 / liquidity
/// With liquidity ≈ 1 and lp_fee ≈ 25, delta ≈ 8.5e39 per swap.
/// After 1000 swaps: fee_growth ≈ 8.5e42 (vs Uint256::MAX ≈ 1.16e77).
/// This won't wrap, but it pushes values high enough to expose
/// overflow bugs in intermediate calculations.
#[test]
fn test_high_fee_growth_accumulation() {
    let mut rng = StdRng::seed_from_u64(421);
    let config = high_fee_growth_config();
    let mut pool = ConcentratedPool::setup(&config);

    // Seed a single tiny position
    pool.seed_liquidity(&mut rng, 1);
    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();

    // Phase 1: Many swaps to push fee_growth high
    let num_swaps = 200;
    for _ in 0..num_swaps {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(100..500),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
    println!(
        "After {} swaps: fee_growth_0={}, fee_growth_1={}",
        num_swaps, snapshot.slot0.fee_growth_global_0_x128, snapshot.slot0.fee_growth_global_1_x128,
    );

    // Phase 2: Add new positions at various ranges — exercises
    // fee_growth_inside computation with large global values
    let ranges = vec![(-100, 100), (-500, -100), (100, 500), (-500, 500)];
    for (lower, upper) in &ranges {
        let op = ConcentratedOp::AddLiquidity {
            lower_tick: *lower,
            upper_tick: *upper,
            amount_0: 500,
            amount_1: 500,
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
            pool.check_snapshot_invariants(&after).assert_all_pass();
        }
    }

    // Phase 3: More swaps to accrue fees for the new positions
    for _ in 0..100 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(100..500),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    // Phase 4: Collect fees from all positions
    let num_positions = pool.positions.len();
    for i in 0..num_positions {
        let before = pool.snapshot();
        let op = ConcentratedOp::CollectFees {
            position_idx: i,
            user_idx: 0,
        };
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    // Phase 5: Remove all positions — tests fees_owed with large inside values
    pool.drain_all_liquidity();
    let snapshot = pool.snapshot();
    pool.check_post_test_invariants(&snapshot).assert_all_pass();
}

// =============================================================================
// Extreme tick boundaries
// =============================================================================

/// Positions at extreme tick ranges near MIN_TICK/MAX_TICK.
/// Tests sqrt_price computation and tick math at boundaries where
/// intermediate calculations are most likely to overflow.
#[test]
fn test_extreme_tick_positions() {
    let mut rng = StdRng::seed_from_u64(1234);
    let config = extreme_tick_config();
    let mut pool = ConcentratedPool::setup(&config);

    // Add positions at extreme ranges
    let extreme_ranges: Vec<(i64, i64)> = vec![
        (-88_720, -88_710), // near MIN boundary
        (88_710, 88_720),   // near MAX boundary
        (-88_720, 88_720),  // full range
        (-88_720, 0),       // half range low
        (0, 88_720),        // half range high
    ];

    for (lower, upper) in &extreme_ranges {
        let op = ConcentratedOp::AddLiquidity {
            lower_tick: *lower,
            upper_tick: *upper,
            amount_0: 10_000,
            amount_1: 10_000,
            user_idx: 0,
        };
        match pool.execute_op(&op) {
            Ok(()) => {
                let snapshot = pool.snapshot();
                pool.check_snapshot_invariants(&snapshot).assert_all_pass();
            }
            Err(e) => {
                println!(
                    "  extreme range [{}, {}] failed (expected): {}",
                    lower, upper, e
                );
            }
        }
    }

    // Swap to cross extreme tick boundaries
    for _ in 0..50 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(5_000..50_000),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();

    // Drain and verify clean state
    pool.drain_all_liquidity();
    let snapshot = pool.snapshot();
    pool.check_post_test_invariants(&snapshot).assert_all_pass();
}

// =============================================================================
// Tick de-initialization and re-initialization cycles
// =============================================================================

/// Creates and removes positions at the same ticks repeatedly.
/// When a tick is de-initialized (removed from storage) and then
/// re-initialized, the fee_growth_outside is reset. This exercises
/// the wrapping math across tick lifecycle boundaries.
#[test]
fn test_tick_reinit_fee_growth() {
    let mut rng = StdRng::seed_from_u64(4567);
    let config = high_fee_growth_config();
    let mut pool = ConcentratedPool::setup(&config);

    let fixed_ticks = (-100i64, 100i64);

    for cycle in 0..5 {
        // Add position at fixed ticks
        let op = ConcentratedOp::AddLiquidity {
            lower_tick: fixed_ticks.0,
            upper_tick: fixed_ticks.1,
            amount_0: 500,
            amount_1: 500,
            user_idx: 0,
        };
        pool.execute_op(&op)
            .unwrap_or_else(|e| panic!("cycle {cycle}: add failed: {e}"));

        // Swap to accrue fees
        for _ in 0..50 {
            let op = ConcentratedOp::Swap {
                amount: rng.gen_range(50..300),
                zero_for_one: rng.gen_bool(0.5),
                user_idx: 0,
            };
            let _ = pool.execute_op(&op);
        }

        let snapshot = pool.snapshot();
        pool.check_snapshot_invariants(&snapshot).assert_all_pass();

        // Collect fees then remove all non-initial positions
        // (keep initial pool position, remove only the one we added)
        let last_idx = pool.positions.len() - 1;
        let _ = pool.execute_op(&ConcentratedOp::CollectFees {
            position_idx: last_idx,
            user_idx: 0,
        });
        let op = ConcentratedOp::RemoveLiquidity {
            position_idx: last_idx,
            fraction_bps: 10_000, // full removal
            user_idx: 0,
        };
        pool.execute_op(&op)
            .unwrap_or_else(|e| panic!("cycle {cycle}: remove failed: {e}"));

        let snapshot = pool.snapshot();
        pool.check_snapshot_invariants(&snapshot).assert_all_pass();
        println!(
            "cycle {}: fee_growth_0={}, positions={}",
            cycle,
            snapshot.slot0.fee_growth_global_0_x128,
            pool.positions.len(),
        );
    }
}

// =============================================================================
// Large value stress test — push intermediate math toward type boundaries
// =============================================================================

/// Uses token amounts near 10^18 to stress intermediate computations:
/// - `lp_fee * 2^128` in accumulate_fee_growth: with lp_fee ≈ 10^16,
///   the product is ≈ 10^16 * 3.4*10^38 = 3.4*10^54 (well within Uint256
///   but exercises the upper range of 256-bit math)
/// - `liquidity * fee_growth_delta / 2^128` in fees_owed: with large
///   liquidity (10^18) and large delta, the numerator pushes toward
///   the upper bits of Uint256
/// - sqrt_price math with large liquidity values
#[test]
fn test_large_value_operations() {
    let mut rng = StdRng::seed_from_u64(3141599);
    let config = large_value_config();
    let mut pool = ConcentratedPool::setup(&config);

    pool.seed_liquidity(&mut rng, 3);
    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();

    // Phase 1: Large swaps
    for _ in 0..50 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(1_000_000_000_000_000..500_000_000_000_000_000u128),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
    println!(
        "Large values: fee_growth_0={}, fee_growth_1={}, reserve_0={}, reserve_1={}",
        snapshot.slot0.fee_growth_global_0_x128,
        snapshot.slot0.fee_growth_global_1_x128,
        snapshot.reserve_0,
        snapshot.reserve_1,
    );

    // Phase 2: Add large positions
    for _ in 0..5 {
        let lower = (rng.gen_range(-500..0i64) / 10) * 10;
        let upper = (rng.gen_range(1..500i64) / 10) * 10;
        let op = ConcentratedOp::AddLiquidity {
            lower_tick: lower,
            upper_tick: upper,
            amount_0: rng.gen_range(1_000_000_000_000_000..100_000_000_000_000_000u128),
            amount_1: rng.gen_range(1_000_000_000_000_000..100_000_000_000_000_000u128),
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
            pool.check_snapshot_invariants(&after).assert_all_pass();
        }
    }

    // Phase 3: More large swaps with more positions
    for _ in 0..50 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(1_000_000_000_000_000..500_000_000_000_000_000u128),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    // Phase 4: Collect fees (large fee amounts)
    let num_positions = pool.positions.len();
    for i in 0..num_positions {
        let before = pool.snapshot();
        let op = ConcentratedOp::CollectFees {
            position_idx: i,
            user_idx: 0,
        };
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();

    // Phase 5: Full drain
    pool.drain_all_liquidity();
    let snapshot = pool.snapshot();
    pool.check_post_test_invariants(&snapshot).assert_all_pass();
}

// =============================================================================
// Minimum liquidity stress test
// =============================================================================

/// Single unit of liquidity with maximum-size swaps.
/// This maximizes fee_growth_delta per swap and tests the extreme
/// end of the fee accumulation math.
#[test]
fn test_minimum_liquidity_max_swaps() {
    let config = ConcentratedConfig {
        fee_tier_bps: 500,
        tick_spacing: 10,
        slippage_tolerance_bps: 10_000,
        initial_amount_0: 100,
        initial_amount_1: 100,
        swap_amount_range: (10, 50),
        add_amount_range: (10, 50),
        seed_amount_range: (10, 50),
        tick_range: (-100, 100),
        ..ConcentratedConfig::default()
    };
    let mut rng = StdRng::seed_from_u64(789);
    let mut pool = ConcentratedPool::setup(&config);

    pool.seed_liquidity(&mut rng, 1);

    // Alternate: swap, add at new ticks, swap, remove, collect
    for i in 0..100 {
        let op = match i % 5 {
            0 | 1 | 2 => ConcentratedOp::Swap {
                amount: rng.gen_range(10..50),
                zero_for_one: rng.gen_bool(0.5),
                user_idx: 0,
            },
            3 => {
                let lower = (rng.gen_range(-10..0i64)) * 10;
                let upper = (rng.gen_range(1..10i64)) * 10;
                ConcentratedOp::AddLiquidity {
                    lower_tick: lower,
                    upper_tick: upper,
                    amount_0: rng.gen_range(10..50),
                    amount_1: rng.gen_range(10..50),
                    user_idx: 0,
                }
            }
            _ => {
                if pool.positions.len() > 1 {
                    ConcentratedOp::RemoveLiquidity {
                        position_idx: pool.positions.len() - 1,
                        fraction_bps: 10_000,
                        user_idx: 0,
                    }
                } else {
                    continue;
                }
            }
        };

        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
            let c2 = crate::invariants::concentrated::check_active_liquidity(&after);
            if !c2.passed {
                panic!(
                    "C2 violated at op {i} ({op:?}): {}\ntick before={}, tick after={}",
                    c2.detail, before.slot0.tick, after.slot0.tick
                );
            }
            let c1 = crate::invariants::concentrated::check_tick_price_consistency(&after);
            if !c1.passed {
                panic!(
                    "C1 violated at op {i} ({op:?}): {}\ntick before={}, tick after={}",
                    c1.detail, before.slot0.tick, after.slot0.tick
                );
            }
        }
    }

    let snapshot = pool.snapshot();
    pool.check_snapshot_invariants(&snapshot).assert_all_pass();
    println!(
        "Final fee_growth: g0={}, g1={}",
        snapshot.slot0.fee_growth_global_0_x128, snapshot.slot0.fee_growth_global_1_x128,
    );
}

// =============================================================================
// Wrapping math integration test — inject near-MAX fee_growth into storage
// =============================================================================

/// Injects fee_growth_global values near Uint256::MAX directly into the VLP
/// contract storage, then performs swaps, adds, removes, and fee collections
/// to verify wrapping arithmetic works end-to-end through the actual contract.
///
/// This is the only way to test wrapping at the integration level — normal
/// operations can't push fee_growth past ~10^40, while MAX is ~10^77.
#[test]
fn test_wrapping_math_with_injected_state() {
    use concentrated_vlp::state::{FEE_GROWTH_GLOBAL_0_X128, FEE_GROWTH_GLOBAL_1_X128, TICKS};
    use cosmwasm_std::Uint256;
    use cw_orch::environment::Environment;

    let mut rng = StdRng::seed_from_u64(271828);
    let config = ConcentratedConfig::default();
    let mut pool = ConcentratedPool::setup(&config);

    // Seed some positions so we have tick state
    pool.seed_liquidity(&mut rng, 3);

    // Do a few swaps to set up realistic tick/fee state
    for _ in 0..10 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(1_000..10_000),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let _ = pool.execute_op(&op);
    }

    // Inject near-MAX fee_growth values directly into contract storage.
    // Set to MAX - 10^40, so the next few swaps will push it past MAX and wrap.
    // Start just 10^35 below MAX — with fee_growth_delta ≈ 10^34 per swap,
    // a few swaps will push past MAX and trigger wrapping.
    let near_max = Uint256::MAX - Uint256::from(10u128).pow(35);
    let vlp_addr = pool.vlp_address();
    {
        let mut app = pool.factory.environment().app.borrow_mut();
        let mut storage = app.contract_storage_mut(&vlp_addr);
        FEE_GROWTH_GLOBAL_0_X128
            .save(storage.as_mut(), &near_max)
            .expect("inject fee_growth_0");
        FEE_GROWTH_GLOBAL_1_X128
            .save(storage.as_mut(), &near_max)
            .expect("inject fee_growth_1");

        // Also update tick fee_growth_outside values proportionally so the
        // invariant below + above <= global still holds approximately.
        // Set all tick outside values to near_max / 2.
        let half_max = near_max / Uint256::from(2u128);
        let tick_entries: Vec<(i64, concentrated_vlp::state::TickInfo)> = TICKS
            .range(storage.as_ref(), None, None, cosmwasm_std::Order::Ascending)
            .filter_map(|r| r.ok())
            .collect();
        for (idx, mut tick) in tick_entries {
            tick.fee_growth_outside_0_x128 = half_max;
            tick.fee_growth_outside_1_x128 = half_max;
            TICKS
                .save(storage.as_mut(), idx, &tick)
                .expect("update tick");
        }
    }

    let snap_after_inject = pool.snapshot();
    assert!(
        snap_after_inject.slot0.fee_growth_global_0_x128 > Uint256::from(10u128).pow(76),
        "fee_growth should be near MAX"
    );

    // Phase 1: Swaps that will cause fee_growth to WRAP past Uint256::MAX
    // Full invariant checks on every successful op.
    println!("Phase 1: Swaps to trigger wrapping...");
    let mut wrapped = false;
    for i in 0..50 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(5_000..30_000),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
            pool.check_snapshot_invariants(&after).assert_all_pass();
            if after.slot0.fee_growth_global_0_x128 < before.slot0.fee_growth_global_0_x128
                || after.slot0.fee_growth_global_1_x128 < before.slot0.fee_growth_global_1_x128
            {
                println!(
                    "  Wrap detected at swap {i}! g0: {} -> {}, g1: {} -> {}",
                    before.slot0.fee_growth_global_0_x128,
                    after.slot0.fee_growth_global_0_x128,
                    before.slot0.fee_growth_global_1_x128,
                    after.slot0.fee_growth_global_1_x128,
                );
                wrapped = true;
            }
        }
    }
    assert!(wrapped, "fee_growth should have wrapped past Uint256::MAX");

    // Phase 2: Add new positions AFTER the wrap — tests fee_growth_inside
    // computation with wrapped accumulators (global is now small, tick
    // outside values are large from pre-wrap era)
    println!("Phase 2: Adding positions after wrap...");
    for _ in 0..3 {
        let lower = (rng.gen_range(-300..0i64) / 10) * 10;
        let upper = (rng.gen_range(1..300i64) / 10) * 10;
        let op = ConcentratedOp::AddLiquidity {
            lower_tick: lower,
            upper_tick: upper,
            amount_0: rng.gen_range(5_000..30_000),
            amount_1: rng.gen_range(5_000..30_000),
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
            pool.check_snapshot_invariants(&after).assert_all_pass();
        }
    }

    // Phase 3: More swaps to accrue fees with wrapped state
    println!("Phase 3: Swaps with wrapped fee_growth...");
    for _ in 0..20 {
        let op = ConcentratedOp::Swap {
            amount: rng.gen_range(1_000..10_000),
            zero_for_one: rng.gen_bool(0.5),
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    // Full snapshot check after all wrapped-state operations
    let snap = pool.snapshot();
    pool.check_snapshot_invariants(&snap).assert_all_pass();

    // Phase 4: Collect fees — this calls fees_owed which must handle
    // the wrapping delta (inside_now - inside_last where inside_now < inside_last
    // due to wrapping, but the wrapping difference is the correct fee amount)
    println!("Phase 4: Collecting fees...");
    let num_positions = pool.positions.len();
    for i in 0..num_positions {
        let op = ConcentratedOp::CollectFees {
            position_idx: i,
            user_idx: 0,
        };
        let before = pool.snapshot();
        if pool.execute_op(&op).is_ok() {
            let after = pool.snapshot();
            pool.check_transition_invariants(&before, &after, &op)
                .assert_all_pass();
        }
    }

    // Phase 5: Remove all positions — tests the full lifecycle with wrapped state
    println!("Phase 5: Draining all positions...");
    pool.drain_all_liquidity();

    let final_snap = pool.snapshot();
    pool.check_post_test_invariants(&final_snap)
        .assert_all_pass();
}
