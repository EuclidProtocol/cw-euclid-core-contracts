# Fuzz Testing Suite

Property-based fuzz testing for Euclid's AMM pools. A generic framework (`FuzzPool` trait + `FuzzRunner`) drives randomized operation sequences against any pool type while checking invariants after every operation. All tests are deterministic and seed-reproducible.

## Architecture

```
tests-fuzz/src/
  runner/
    mod.rs               # Generic FuzzRunner<P> and FuzzPool trait
    stats.rs             # RunStats (per-op timing and counts)
    coverage.rs          # InvariantCoverage (per-invariant check counts)
  invariants/
    mod.rs               # InvariantCheck + InvariantResult
  harness/
    mod.rs
    util.rs              # root_cause() CwOrchError helper
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
- **`run_linear(positions, ops)`** — Phased: seed positions -> random ops -> drain all -> assert clean state.
- **`run_mixed_timed(duration, report_interval)`** — Time-boxed single pool: two-tier invariant checking (light every op, full every Nth op) for sustained throughput.
- **`run_for_duration(config, secs, ops, seed, positions)`** — Time-boxed multi-pool: spawns fresh pools with unique seeds until wall-clock expires.

### `RunStats` (`runner/stats.rs`)

Tracks per-operation success/failure counts and wall-clock timing (min/avg/max/stddev). Printed at the end of each run for debugging operation distribution and performance.

### `InvariantCoverage` (`runner/coverage.rs`)

Counts how many times each invariant was checked during a run. Reveals which invariants are undertested.

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

The `FuzzRunner` and all run modes (`run_mixed`, `run_linear`, `run_mixed_timed`, `run_for_duration`) work automatically.
