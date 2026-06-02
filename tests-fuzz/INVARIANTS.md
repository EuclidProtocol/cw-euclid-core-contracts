# Invariant Reference

All invariants checked by the fuzz testing suite. Each invariant is a property that must hold at specific points during pool operation.

## When Invariants Are Checked

| Timing | Invariants | Trigger |
|--------|-----------|---------|
| After every operation | Snapshot (C1-C7, C9, S1) | `check_snapshot_invariants()` |
| After every successful operation | Transition (T1-T8, S2-S3) | `check_transition_invariants()` |
| After draining all positions | Post-test (P1-P2) | `check_post_test_invariants()` |

## Snapshot Invariants (State-at-Rest)

These must hold at any point in time, regardless of what operations have been performed.

### Concentrated Liquidity (C1-C7, C9)

| ID | Name | Property | Source |
|----|------|----------|--------|
| C1 | `tick_price_consistency` | `tick_at_sqrt_ratio(slot0.sqrt_price_x96) == slot0.tick` (within +-1) | KyberSwap exploit finding |
| C2 | `active_liquidity` | `slot0.liquidity == sum(pos.liquidity for positions where lower <= tick < upper)` | Certora formal verification |
| C3 | `tick_liquidity_gross` | For each initialized tick: `tick.liquidity_gross == sum(pos.liquidity for positions touching that tick)` | Trail of Bits Props #2-3, #8-9 |
| C4 | `liquidity_net_sum_zero` | `sum(tick.liquidity_net for all initialized ticks) == 0` | Trail of Bits Prop #20, Velodrome Prop #20 |
| C5 | `position_bounds` | For each position: `lower_tick < upper_tick` and both aligned to `tick_spacing` | Basic CLP constraint |
| C6 | `fee_growth_inside` | Fee growth inside each position is a forward delta from `fee_growth_inside_last` (wrapping math) | Uniswap V3 accumulator design |
| C7 | `sqrt_price_bounds` | `MIN_SQRT_RATIO <= slot0.sqrt_price_x96 < MAX_SQRT_RATIO` | Uniswap V3 tick math bounds |
| C9 | `reserve_solvency` | Reserves >= sum(tokens_owed) + protocol_fees | AMM accounting correctness |

### Shared (S1)

| ID | Name | Property | Source |
|----|------|----------|--------|
| S1 | `reserves_consistent` | If active liquidity > 0, at least one reserve is non-zero | Sanity check |

## Transition Invariants (Before/After)

Checked by comparing snapshots before and after each successful operation.

### Shared (S2-S3)

| ID | Name | Property | Source |
|----|------|----------|--------|
| S2 | `fee_growth_monotonic` | `fee_growth_global_0_x128` and `fee_growth_global_1_x128` never decrease (wrapping-aware) | Fee accumulator monotonicity |
| S3 | `protocol_fees_monotonic` | `protocol_fees.amount_0` and `protocol_fees.amount_1` never decrease | Protocol fee accumulator monotonicity |

### Per-Operation (T1-T8)

| ID | Name | Property | Applies To | Source |
|----|------|----------|-----------|--------|
| T1 | `swap_fee_growth` | Fee growth for the input token increases; output token fee growth unchanged | Swap | Trail of Bits Props #13-16 |
| T2 | `swap_output_bounded` | Swap output < output reserve before swap | Swap | Basic AMM constraint |
| T3 | `tick_liquidity_correlation` | If tick doesn't change, active liquidity doesn't change | Swap | Trail of Bits Prop #17 |
| T4 | `mint_active_liquidity` | In-range mint increases `slot0.liquidity`; out-of-range mint doesn't change it | AddLiquidity | Trail of Bits Prop #1 |
| T5 | `burn_active_liquidity` | In-range burn decreases `slot0.liquidity`; out-of-range burn doesn't change it | RemoveLiquidity | Trail of Bits Prop #7 |
| T6 | `swap_reserve_conservation` | Input reserve increases and output reserve decreases | Swap | AMM conservation |
| T7 | `swap_price_direction` | zero_for_one pushes price down; !zero_for_one pushes price up | Swap | Price monotonicity |
| T8 | `swap_protocol_fees` | Protocol fees for the input token increase | Swap | Fee split correctness |

## Post-Test Invariants (After Drain)

Checked after all positions are removed from the pool.

| ID | Name | Property | Source |
|----|------|----------|--------|
| P1 | `clean_ticks` | No initialized ticks with non-zero `liquidity_gross` remain | Full drain correctness |
| P2 | `clean_removal` | `slot0.liquidity == 0` after all positions removed | Full drain correctness |

## Invariant Check Flow

```
run_mixed(N):
  for each op in 0..N:
    before = snapshot()
    result = execute_op(random_op)
    if result.ok:
      after = snapshot()
      assert check_transition_invariants(before, after, op)  # T1-T8, S2, S3
      assert check_snapshot_invariants(after)                 # C1-C7, C9, S1
    else:
      assert check_snapshot_invariants(before)                # state unchanged

run_linear(positions, ops):
  seed_liquidity(positions)
  assert check_snapshot_invariants()                          # C1-C7, C9, S1
  ... same per-op loop as run_mixed ...
  drain_all_liquidity()
  assert check_post_test_invariants()                         # P1, P2
```

## Adding New Invariants

1. Add the check function in the appropriate file (`invariants/concentrated/snapshot.rs`, `invariants/shared.rs`, etc.)
2. Return `InvariantCheck::pass(name)` or `InvariantCheck::fail(name, detail)`
3. Wire it into the composite checker (`check_all_snapshot_invariants`, `check_shared_transition`, etc.)
4. Update this document

## References

- [Uniswap V3 Core Invariants](https://github.com/Uniswap/v3-core) -- original tick math, fee growth accumulator design
- [Trail of Bits Uniswap V3 Properties](https://github.com/crytic/properties) -- Props #1-20
- [Certora Formal Verification](https://www.certora.com/) -- active liquidity sum property
- [KyberSwap Exploit Analysis](https://blog.kyberswap.com/) -- tick/price consistency finding
- [Velodrome V2 Properties](https://github.com/velodrome-finance/) -- liquidity_net sum property
