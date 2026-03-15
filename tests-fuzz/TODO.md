# Fuzz Testing TODO

Tracked improvements for the tests-fuzz crate. Not committed to git.

## High Priority

### 1. Wire T1-T5 per-operation transition invariants
- **Status**: Invariants implemented (`invariants/concentrated.rs`), marked `#[allow(dead_code)]`
- **Problem**: `check_transition_invariants` doesn't receive the operation, so it can't dispatch to T1-T5 by op type. Only shared S2/S3 (fee monotonicity) are checked.
- **Plan**:
  - Add `op: &Self::Op` parameter to `FuzzPool::check_transition_invariants()`
  - Update `execute_checked()` in runner to pass the op through
  - In `ConcentratedPool` impl, dispatch: swap -> T1+T2+T3, add -> T4, remove -> T5
  - Remove `#[allow(dead_code)]` from T1-T5 functions
- **Impact**: These catch real bugs — fee accounting errors, output bound violations, liquidity tracking inconsistencies

### 2. Re-enable C6 (fee_growth_inside_consistency)
- **Status**: Disabled in `invariants/concentrated.rs` pending PR #138
- **Problem**: Contract uses `checked_sub` where wrapping math is needed, causing false negatives during tick re-initialization
- **Plan**: Check if PR #138 has landed; if so, re-enable C6
- **Impact**: Fee accounting bugs are the most common CLP vulnerability class

## Medium Priority

### 3. Failure sequence recording and replay
- **Status**: Not implemented. Failures print seed + op_idx to stdout only.
- **Plan**:
  - On invariant violation, serialize the operation sequence to `tests-fuzz/regressions/<seed>.json`
  - Add a `#[test] fn replay_regressions()` that reads and replays all regression files
  - Regression files become permanent CI tests
- **Impact**: Prevents regressions from being lost in CI logs, builds a corpus over time

### 4. Invariant coverage tracking
- **Status**: No visibility into which invariants are actually being stressed
- **Problem**: An invariant that always trivially passes (e.g., C2 with 1 position) isn't testing anything
- **Plan**:
  - Add counters to `InvariantResult`: times checked, times "near boundary"
  - Define "near boundary" per invariant (e.g., liquidity delta < 1% of total, reserves near zero)
  - Print coverage report in `print_summary()` alongside op stats
- **Impact**: Identifies blind spots in strategy generation

### 5. Config validation at runtime
- **Status**: Weight sum validated with `debug_assert_eq!` only (doesn't fire in release)
- **Plan**: Use `assert_eq!` or add `ConcentratedConfig::validate()` called from `setup()`
- **Impact**: Low effort, prevents silent misconfiguration

## Lower Priority

### 6. Widen strategy coverage for known attack vectors
- **Price manipulation sequences**: large swaps followed by small add/removes at manipulated prices
- **Tick boundary positions**: lower_tick or upper_tick exactly at current_tick (in-range boundary)
- **Zero-liquidity tick crossings**: swaps crossing ticks where all positions removed but tick state remains
- **Max position density**: many narrow positions on adjacent ticks stressing tick traversal
- Could be new biases in `ConcentratedOp::random()` or new targeted test scenarios

### 7. Gas profiling via test-tube
- **Status**: Plan file exists (`tests-fuzz/PLAN-gas-profiling.md`)
- **Plan**: Integrate test-tube for real gas metering alongside wall-clock timing
- **Blocked on**: test-tube compatibility with current contract setup

## Not Planned (and why)

- **Parallel seed execution**: cw-multi-test isn't thread-safe
- **Coverage-guided fuzzing**: State-aware strategies are more effective for CLP domain than code coverage guidance
- **Automatic shrinking**: Seed + op_idx gives deterministic replay; manual bisection is sufficient for now
