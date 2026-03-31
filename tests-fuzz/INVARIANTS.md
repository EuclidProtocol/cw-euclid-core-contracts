# Invariant Reference

All invariants checked by the fuzz testing suite. Each invariant is a property that must hold at specific points during pool operation.

## When Invariants Are Checked

| Timing | Invariants | Trigger |
|--------|-----------|---------|
| After every operation | Snapshot (C1-C8, S1) | `check_snapshot_invariants()` |
| After every successful operation | Transition (S2-S3) | `check_transition_invariants()` |
| After draining all positions | Post-test (P1-P2) | `check_post_test_invariants()` |
| Per-operation (planned) | Operation-specific (T1-T5) | Not yet wired into runner |

## Snapshot Invariants (State-at-Rest)

These must hold at any point in time, regardless of what operations have been performed.

### Concentrated Liquidity (C1-C8)

| ID | Name | Property | Source |
|----|------|----------|--------|
| C1 | `tick_price_consistency` | `tick_at_sqrt_ratio(slot0.sqrt_price_x96) == slot0.tick` | KyberSwap exploit finding |
| C2 | `active_liquidity` | `slot0.liquidity == sum(pos.liquidity for positions where lower <= tick < upper)` | Certora formal verification |
| C3 | `tick_liquidity_gross` | For each initialized tick: `tick.liquidity_gross == sum(pos.liquidity for positions touching that tick)` | Trail of Bits Props #2-3, #8-9 |
| C4 | `liquidity_net_sum_zero` | `sum(tick.liquidity_net for all initialized ticks) == 0` | Trail of Bits Prop #20, Velodrome Prop #20 |
| C5 | `position_bounds` | For each position: `lower_tick < upper_tick` and both aligned to `tick_spacing` | Basic CLP constraint |
| C6 | `fee_growth_inside` | Fee growth inside each position is a forward delta from `fee_growth_inside_last` (wrapping math). **Currently disabled** — contract uses `checked_sub`; enable after wrapping math lands (PR #138). | Uniswap V3 accumulator design |
| C7 | `sqrt_price_bounds` | `MIN_SQRT_RATIO <= slot0.sqrt_price_x96 < MAX_SQRT_RATIO` | Uniswap V3 tick math bounds |
| C8 | `unlocked` | `slot0.unlocked == true` (reentrancy guard not stuck) | Reentrancy protection |

### Shared (S1)

| ID | Name | Property | Source |
|----|------|----------|--------|
| S1 | `reserves_valid` | Reserves are queryable (Uint128 guarantees non-negative) | Sanity check |

## Transition Invariants (Before/After)

Checked by comparing snapshots before and after each successful operation.

### Shared (S2-S3)

| ID | Name | Property | Source |
|----|------|----------|--------|
| S2 | `fee_growth_monotonic` | `fee_growth_global_0_x128` and `fee_growth_global_1_x128` never decrease | Fee accumulator monotonicity |
| S3 | `protocol_fees_monotonic` | `protocol_fees.amount_0` and `protocol_fees.amount_1` never decrease | Protocol fee accumulator monotonicity |

### Per-Operation (T1-T5) — Planned

These invariants require knowledge of which operation was performed. They are implemented but not yet wired into the generic runner's transition check pipeline.

| ID | Name | Property | Applies To | Source |
|----|------|----------|-----------|--------|
| T1 | `swap_fee_growth` | Fee growth for the input token increases; output token fee growth unchanged | Swap | Trail of Bits Props #13-16 |
| T2 | `swap_output_bounded` | Swap output < output reserve before swap | Swap | Basic AMM constraint |
| T3 | `tick_liquidity_correlation` | If tick doesn't change, active liquidity doesn't change | Swap | Trail of Bits Prop #17 |
| T4 | `mint_active_liquidity` | In-range mint increases `slot0.liquidity`; out-of-range mint doesn't change it | AddLiquidity | Trail of Bits Prop #1 |
| T5 | `burn_active_liquidity` | In-range burn decreases `slot0.liquidity`; out-of-range burn doesn't change it | RemoveLiquidity | Trail of Bits Prop #7 |

## Post-Test Invariants (After Drain)

Checked after all positions are removed from the pool.

| ID | Name | Property | Source |
|----|------|----------|--------|
| P1 | `clean_removal` | `slot0.liquidity == 0` after all positions removed | Full drain correctness |
| P2 | `clean_ticks` | No initialized ticks with non-zero `liquidity_gross` remain | Full drain correctness |

## Invariant Check Flow

```
run_mixed(N):
  for each op in 0..N:
    before = snapshot()
    result = execute_op(random_op)
    if result.ok:
      after = snapshot()
      assert check_transition_invariants(before, after)  # S2, S3
      assert check_snapshot_invariants(after)             # C1-C8, S1
    else:
      assert check_snapshot_invariants(before)            # state unchanged

run_linear(positions, ops):
  seed_liquidity(positions)
  assert check_snapshot_invariants()                      # C1-C8, S1
  ... same per-op loop as run_mixed ...
  drain_all_liquidity()
  assert check_post_test_invariants()                     # P1, P2
```

## Adding New Invariants

1. Add the check function in the appropriate file (`invariants/concentrated.rs`, `invariants/shared.rs`, or a new file)
2. Return `InvariantCheck::pass(name)` or `InvariantCheck::fail(name, detail)`
3. Wire it into the composite checker (`check_all_snapshot_invariants`, `check_shared_transition`, etc.)
4. For per-operation invariants (T-series), implement the function and add it to the composite checker once the runner supports op-aware transition checks

## References

- [Uniswap V3 Core Invariants](https://github.com/Uniswap/v3-core) — original tick math, fee growth accumulator design
- [Trail of Bits Uniswap V3 Properties](https://github.com/crytic/properties) — Props #1-20
- [Certora Formal Verification](https://www.certora.com/) — active liquidity sum property
- [KyberSwap Exploit Analysis](https://blog.kyberswap.com/) — tick/price consistency finding
- [Velodrome V2 Properties](https://github.com/velodrome-finance/) — liquidity_net sum property
