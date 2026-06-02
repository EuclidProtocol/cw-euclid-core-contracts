# SC-23 Issue 8 — CLP Euclid-fee override (issue breakdown)

Vertical-slice breakdown of "Extend SC-23 Euclid-Fee Override to the Concentrated-Liquidity Pool (CLP)".

- **Parent:** SC-23 — Adding custom logic to exempt MM wallet address from Euclid fees
- **Team / project / milestone:** Smart Contracts · Contracts Refactor · M1 — Critical Refactors & Stability Fixes
- **Label (all slices):** `feature`
- **Type:** all AFK — the `resolve_effective_fee` mapping is fully specified, no design hand-off or architectural decision needed.

> The rebase onto `gc/clp` and Issues 1–7 are already committed on `joe/sc-23-euclid-fee-override`. The shared `VlpSwapMsg.euclid_fee_override` field already reaches `execute_clp_swap` — CLP currently passes `euclid_fee_override: None` (plumbed but ignored). These slices cover only the remaining Issue 8 work.

---

## Slice 1 — CLP single-hop Euclid-fee override (core mapping)

**Type:** AFK · **Label:** `feature`

### Parent

SC-23 — Adding custom logic to exempt MM wallet address from Euclid fees

### What to build

The tracer bullet for CLP fee overrides. Introduce a `resolve_effective_fee` mapping that converts a wallet's `euclid_fee_override` (bps) into a per-call `(effective_fee_pips, effective_protocol_cut_bps)` pair, and thread the override from the swap message into the concentrated-pool swap math so a single-hop CLP swap actually reduces what the trader pays — not just the protocol's slice.

The override is read from `VlpSwapMsg` on the execute path and from `VlpSimulateSwapMsg` on the single-hop simulate path. Both paths share the same swap-simulation routine, so they apply the override identically.

The mapping (the spec — do not paper over divergence with rounding):

```
tier_pips                = pool's structural fee tier (pips)
default_protocol_cut_bps = pool's configured Euclid-fee bps (reinterpreted as protocol cut)
lp_pips                  = tier_pips * (10_000 - default_protocol_cut_bps) / 10_000

with override X (bps):
  effective_fee_pips          = lp_pips + X * 100
  effective_protocol_cut_bps  = (X * 100 * 10_000) / effective_fee_pips   // floor
```

Invariants the mapping must satisfy:
- `None` or `Some(default_protocol_cut_bps)` → identity: `effective_fee_pips == tier_pips`, `effective_protocol_cut_bps == default_protocol_cut_bps`. Safe no-op for wallets without an override.
- `Some(0)` → trader pays only `lp_pips`; protocol cut is 0; LP per-unit accrual unchanged.
- `0 < X < default` → partial trader discount; LP per-step accrual unchanged in absolute pips; protocol slice reduced.

Also refresh the deferred-CLP TODO in the pool package so it points at this implementation and documents the `lp_pips + X*100` invariant (CLP does not route through `pre_swap` — do not branch on the override there). Add `[concentrated_vlp]` and `[euclid-pool]` CHANGELOG entries under the current in-progress section.

### Acceptance criteria

- [ ] `resolve_effective_fee(tier_pips, default_protocol_cut_bps, euclid_fee_override)` exists and returns `(effective_fee_pips, effective_protocol_cut_bps)` per the mapping above.
- [ ] `euclid_fee_override` is read from `VlpSwapMsg` (execute) and `VlpSimulateSwapMsg` (single-hop simulate) and threaded into the shared swap-simulation routine.
- [ ] Unit truth table passes: `None` → identity; `Some(default)` → identity; `Some(0)` → `(lp_pips, 0)`; `Some(default/2)` → midpoint.
- [ ] Per-step swap test: protocol-fee accrual is reduced exactly as the override predicts, while `fee_growth_global_*_x128` per unit liquidity is unchanged across override vs. no-override at the same tier.
- [ ] Multi-tick swap test: the override applies uniformly across every tick step (no first-step-only bug).
- [ ] Invariant test: at any override value, the trader's effective received amount ≥ the unrebated quote, and LP fee accrual ≥ unrebated LP accrual.
- [ ] Mixed-version decode test: a `concentrated_vlp` built against the pre-override message decodes a new-shape message via `#[serde(default)]`.
- [ ] Deferred-CLP TODO in the pool package is refreshed; CHANGELOG `[concentrated_vlp]` + `[euclid-pool]` entries added.
- [ ] `cargo test -p concentrated_vlp`, `cargo clippy --all-targets --all-features`, and `cargo fmt --check` are clean; existing tests untouched.

### Blocked by

None — can start immediately.

---

## Slice 2 — Multi-hop override forwarding through CLP simulate

**Type:** AFK · **Label:** `feature`

### Parent

SC-23 — Adding custom logic to exempt MM wallet address from Euclid fees

### What to build

Make the override survive multi-hop routing that passes *through* a CLP pool. The CLP simulate query forwards the wallet's `euclid_fee_override` onward to the next swaps in the route, matching the forwarding pattern already established for the constant-product / stable pools. A route like `cp → CLP → stable` then applies the same wallet's override on every hop, and the simulate quote matches execution.

### Acceptance criteria

- [ ] The CLP simulate query forwards `euclid_fee_override` to the next-hop swaps (same pattern as the cp/stable simulate forwarding).
- [ ] Integration test (mixed multi-hop, reusing the existing mixed-concentrated swap pattern): a `cp → CLP → stable` route applies the same wallet's override on every hop.
- [ ] The multi-hop simulate quote matches the executed `receive_amount`.
- [ ] `cargo clippy --all-targets --all-features` and `cargo fmt --check` clean.

### Blocked by

- Slice 1 — CLP single-hop Euclid-fee override (core mapping)

---

## Slice 3 — CLP override integration matrix + LP-collect invariant

**Type:** AFK · **Label:** `feature`

### Parent

SC-23 — Adding custom logic to exempt MM wallet address from Euclid fees

### What to build

Full integration coverage for the CLP override across all chain modes (Native / IBC / EVM), mirroring the existing concentrated-pool fee/swap integration tests. An admin sets an override for a test wallet on a CLP route; the test then verifies the swap quote and execution agree, the protocol-fee accrual is zero/reduced as the override predicts, and LP fee collection (`CollectFees`) is unaffected per unit liquidity.

### Acceptance criteria

- [ ] CLP analogue of the existing concentrated fees/swap integration tests, parameterized across Native/IBC/EVM modes.
- [ ] Test covers: (a) admin sets override on a CLP route; (b) swap quote and execution match; (c) protocol-fee accrual is zero / reduced as expected; (d) `CollectFees` LP payout is unaffected per unit liquidity.
- [ ] `cargo test -p tests-integration concentrated` matrix is green.
- [ ] `cargo clippy --all-targets --all-features` and `cargo fmt --check` clean.

### Blocked by

- Slice 1 — CLP single-hop Euclid-fee override (core mapping)
