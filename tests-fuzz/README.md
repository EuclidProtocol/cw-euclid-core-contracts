# Fuzz Testing Suite

Property-based fuzz testing for Euclid's AMM pools. A generic framework (`FuzzPool` trait + `FuzzRunner`) drives randomized operation sequences against any pool type while checking invariants after every operation. All tests are deterministic and seed-reproducible.

Currently implemented: **Concentrated Liquidity Pools (CLP)**. The framework is designed for constant product and stable swap pools to be added with the same harness pattern.

## Architecture

```
tests-fuzz/src/
  runner/
    mod.rs               # Generic FuzzRunner<P> and FuzzPool trait
    stats.rs             # RunStats (per-op timing, counts, error breakdown)
    coverage.rs          # InvariantCoverage (per-invariant check counts)
  harness/
    concentrated.rs      # ConcentratedPool — FuzzPool impl for CLP
    util.rs              # root_cause() CwOrchError helper
  strategies/
    concentrated.rs      # ConcentratedOp enum + state-aware random generation
  invariants/
    mod.rs               # PoolSnapshot, InvariantResult, InvariantCheck
    concentrated.rs      # CLP-specific invariants (C1-C8, T1-T5, P1-P2)
    shared.rs            # Cross-pool invariants (S1-S3)
  tests/
    concentrated/
      mixed.rs           # Random-operation fuzz runs
      linear.rs          # Seed → operate → drain lifecycle
      targeted.rs        # Specific edge-case / attack scenarios
      multiuser.rs       # Multi-user concurrency, MEV, authorization
  math/                  # Property tests for low-level CLP math functions
  helpers/               # Re-exports from test_helpers (chain setup, factory ops)
```

## Generic Framework

### `FuzzPool` trait (`runner/mod.rs`)

Any pool type implements this trait to plug into the generic runner:

| Method | Purpose |
|--------|---------|
| `setup(config)` | Deploy contracts, create pool, fund users |
| `random_op(rng)` | Generate a random operation from live pool state |
| `execute_op(op)` | Execute an operation, return `Ok(())` or `Err(msg)` |
| `snapshot()` | Capture full pool state for invariant checking |
| `light_snapshot()` | Capture lightweight state (default: falls back to `snapshot()`) |
| `check_snapshot_invariants(snap)` | Verify state-at-rest properties |
| `check_light_snapshot_invariants(snap)` | Verify cheap invariants (default: falls back to full check) |
| `check_transition_invariants(before, after, op)` | Verify before/after properties |
| `check_post_test_invariants(snap)` | Verify clean state after drain |
| `seed_liquidity(rng, n)` | Add initial liquidity as starting state |
| `drain_all_liquidity()` | Remove all liquidity positions |
| `op_name(op)` | Human-readable operation name for stats |
| `pool_name()` | Human-readable pool description for logging |

Associated types: `Op` (operation enum), `Snapshot` (pool state capture), `Config` (tunable parameters).

### `FuzzRunner<P: FuzzPool>` (`runner/mod.rs`)

Generic runner that orchestrates fuzz campaigns for any pool type:

- **`run_mixed(num_ops)`** — Random operations with snapshot + transition invariant checks after every op. Panics on first violation with full context (op index, op details, seed).
- **`run_linear(positions, ops)`** — Phased: seed positions -> random ops -> drain all -> assert clean state (P1, P2).
- **`run_mixed_timed(duration, report_interval)`** — Time-boxed single pool: two-tier invariant checking (light every op, full every Nth op) for sustained throughput.
- **`run_for_duration(config, secs, ops, seed, positions)`** — Time-boxed multi-pool: spawns fresh pools with unique seeds until wall-clock expires.

### `RunStats` (`runner/stats.rs`)

Tracks per-operation success/failure counts and wall-clock timing (min/avg/max/stddev). Includes error breakdown by (operation, message) for diagnosing failure patterns. Printed at the end of each run for debugging operation distribution and performance.

### `InvariantCoverage` (`runner/coverage.rs`)

Counts how many times each invariant was checked during a run. Reveals which invariants are undertested.

### Shared Invariants (`invariants/shared.rs`)

Cross-pool invariants that apply regardless of pool type:

| ID | Name | Property |
|----|------|----------|
| S1 | `reserves_valid` | Reserves are queryable and non-negative |
| S2 | `fee_growth_monotonic` | Global fee growth accumulators never decrease |
| S3 | `protocol_fees_monotonic` | Protocol fee accumulators never decrease |

## Concentrated Liquidity Implementation

### Harness: `ConcentratedPool` (`harness/concentrated.rs`)

Implements `FuzzPool` for Euclid's Uniswap V3-style concentrated liquidity pool.

**Configuration** (`ConcentratedConfig`):
- Pool params: `fee_tier_bps`, `tick_spacing`, `slippage_tolerance_bps`
- Initial amounts: `initial_amount_0`, `initial_amount_1`
- Operation weights: `weight_swap`, `weight_add`, `weight_remove`, `weight_collect` (must sum to 100)
- Amount/tick ranges for random generation
- `num_users`: 1 for single-user, >1 for multi-user mode

**Multi-user support**:
- User 0 is the pool creator (environment default sender)
- Additional users created via `addr_make` and funded with both tokens
- `with_user(idx, closure)` / `with_sender(addr, closure)` — switches factory sender for closure duration, guarantees restore
- `resolve_user_position(user_idx, relative_idx)` — maps user-relative position index to global
- `random_op` distributes across users with user-relative position counts when multi-user

### Strategy: `ConcentratedOp` (`strategies/concentrated.rs`)

Four operation variants: `Swap`, `AddLiquidity`, `RemoveLiquidity`, `CollectFees`. Each carries a `user_idx`. State-aware generation:

- Swaps biased toward directions with reserves, capped at 90% of output reserves
- AddLiquidity generates 80% in-range positions (spanning current tick)
- Remove/Collect fall back to AddLiquidity when no positions exist
- Integer math for tick-proportional amount splitting

### CLP-specific Invariants

See [INVARIANTS.md](INVARIANTS.md) for the full invariant reference (C1-C8, T1-T5, P1-P2).

## Test Categories

### Concentrated: Mixed (`concentrated/mixed.rs`) — 4 tests
Random operation sequences with configurable tick ranges and duration. Invariants checked after every operation.

### Concentrated: Linear (`concentrated/linear.rs`) — 3 tests
Full lifecycle: seed positions -> operate -> drain. Validates clean state (P1, P2).

### Concentrated: Targeted (`concentrated/targeted.rs`) — 7 tests
Specific edge cases: dust swaps, tick boundary crossings, round-trip value loss, overlapping positions, empty pool behavior, single-spacing positions.

### Concentrated: Multi-user (`concentrated/multiuser.rs`) — 8 tests
- **Runner-based** (3): Mixed fuzz, linear lifecycle, concurrent liquidity with drain
- **Scenario-based** (3): Sandwich attack, cross-user fee isolation, JIT liquidity
- **Authorization** (2): Unauthorized remove, unauthorized fee collection

### Math (`math/`) — 30 tests
Property tests for tick math, sqrt price math, swap math, full math, position math, and liquidity amount calculations.

## Running Tests

```bash
# All fuzz tests
cargo test -p tests-fuzz

# By pool type
cargo test -p tests-fuzz -- concentrated

# By category
cargo test -p tests-fuzz -- concentrated::mixed
cargo test -p tests-fuzz -- concentrated::multiuser
cargo test -p tests-fuzz -- concentrated::targeted
cargo test -p tests-fuzz -- concentrated::linear
cargo test -p tests-fuzz -- math

# Single test
cargo test -p tests-fuzz -- test_sandwich_attack_pattern
```

## Reproducibility

All randomness uses `StdRng` seeded from deterministic `u64` values. When a test fails, the panic message includes the seed:

```
Snapshot invariant violated at op 42 (Swap { amount: 1234, ... }, seed=314):
  C2:active_liquidity — slot0.liquidity=500 != sum(in-range positions)=450
```

Re-run with the same seed to reproduce exactly.

## Adding a New Pool Type

1. **Harness**: `harness/my_pool.rs` — struct implementing `FuzzPool`
2. **Strategy**: `strategies/my_pool.rs` — operation enum with `random()` generation
3. **Invariants**: `invariants/my_pool.rs` — pool-specific snapshot/transition/post-test checks
4. **Tests**: `tests/my_pool/` — test modules using `FuzzRunner<MyPool>`

The `FuzzRunner` and all run modes (`run_mixed`, `run_linear`, `run_mixed_timed`, `run_for_duration`) work automatically. Shared invariants (S1-S3) apply to all pool types via `invariants/shared.rs`.
