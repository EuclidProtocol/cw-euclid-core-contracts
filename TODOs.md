# CLP V3 Implementation Tracker (ClickUp-Style)

Last Updated: 2026-02-24
Owner: Core Protocol Team
Scope: Concentrated VLP parity, cross-chain integration, migration hardening

## How To Use
- Status values: `Backlog`, `In Progress`, `Blocked`, `Done`
- Priority values: `P0`, `P1`, `P2`
- Move checklist items as work progresses.
- Each task is done only when code, tests, and docs are complete.

## Board

### P0-001: Replace LP-share mint/burn with true V3 position liquidity math
- Status: `Done`
- Priority: `P0`
- Why:
  - Current position liquidity is tied to generic LP-share minting/removal, not true V3 `liquidity` computation from range and current price.
- Deliverables:
  - Implement V3-equivalent mint logic:
    - `liquidity = min(liquidity_from_amount0, liquidity_from_amount1)` based on `(sqrtP, sqrtA, sqrtB)`.
  - Implement burn logic using liquidity delta -> exact token amounts by current price and range.
  - Remove dependency on generic pool add/remove math for concentrated position accounting.
- Tasks:
  - [x] Add `liquidity_amounts` math helpers (`get_liquidity_for_amount0/1`, `get_amount0/1_for_liquidity`).
  - [x] Refactor concentrated `AddLiquidity` to compute position liquidity directly.
  - [x] Refactor concentrated `RemoveLiquidity` to compute owed token deltas directly.
  - [x] Keep backward-compatible response fields.
- Tests:
  - [x] Unit vectors for liquidity/amount conversion around in-range, below-range, above-range.
  - [x] Integration: add/remove parity with deterministic fixtures (integer exact).
  - [x] Regression: existing concentrated position tests all green.
- Done Criteria:
  - [x] `position.liquidity` equals V3 liquidity units, not LP shares.
  - [x] Add/remove amounts match V3 math expectations in tests.

### P0-002: Wire concentrated fee collection end-to-end via factory/router IBC
- Status: `Done`
- Priority: `P0`
- Why:
  - Factory and router still return `NotImplemented` for concentrated collect flows.
- Deliverables:
  - Factory execute handlers for:
    - `CollectConcentratedFees`
    - `CollectConcentratedProtocolFees`
  - Router IBC receive handlers for both collect routes.
  - Ack handling and idempotency for collect responses.
- Tasks:
  - [x] Implement factory execute dispatch paths in `factory/src/contract.rs`.
  - [x] Implement router receive path in `router/src/ibc/receive/base.rs`.
  - [x] Add pending request maps and ack handlers in `factory/src/ibc/ack_and_timeout.rs`.
  - [x] Ensure owner/admin authorization checks are preserved across chains.
- Tests:
  - [x] Integration: collect position fees native + IBC.
  - [x] Integration: collect protocol fees admin path native + IBC.
  - [x] Integration: duplicate ack idempotency.
  - [x] Integration: ack error rollback / refund behavior.
- Done Criteria:
  - [x] No `NotImplemented` remains for concentrated collect execute paths.
  - [x] Collect flows succeed and are idempotent across transport modes.

### P0-003: Full migration of legacy concentrated pools to true V3 position/tick state
- Status: `Done`
- Priority: `P0`
- Why:
  - Migration currently initializes global runtime state but does not fully reconstruct per-position/per-tick fee snapshots and liquidity semantics.
- Deliverables:
  - Version-gated migration that reconstructs:
    - position liquidity semantics
    - tick gross/net state
    - fee-growth snapshots for positions
  - Deterministic residual handling policy.
- Tasks:
  - [x] Define migration mapping from legacy fields -> V3 fields.
  - [x] Rebuild tick states for all positions.
  - [x] Initialize position fee snapshots consistently.
  - [x] Add idempotency guard and rerun safety.
- Tests:
  - [x] Unit migration fixtures with edge cases.
  - [x] Integration migration scenario: pre-upgrade pool + post-upgrade swap/collect.
  - [x] Ownership and position ID continuity checks.
- Done Criteria:
  - [x] Post-migration swaps and collects behave correctly on migrated pools.
  - [x] No orphaned or inconsistent position/tick state after migration.

### P1-004: Bitmap-driven next initialized tick search
- Status: `Backlog`
- Priority: `P1`
- Why:
  - Current swap path scans the `TICKS` map range; this is less scalable and diverges from canonical bitmap lookup behavior.
- Deliverables:
  - Implement word-level bitmap traversal for next initialized tick forward/backward.
  - Replace map range scan path in swap loop.
- Tasks:
  - [ ] Add `next_initialized_tick_within_one_word` helpers.
  - [ ] Implement multi-word traversal with bounds.
  - [ ] Integrate into swap loop.
- Tests:
  - [ ] Unit tests for forward/backward lookup across word boundaries.
  - [ ] Contract tests for multi-tick crossing with sparse initialization.
- Done Criteria:
  - [ ] Swap loop no longer depends on `TICKS.range` scanning for tick discovery.

### P1-005: Exact-output swaps for concentrated pools
- Status: `Backlog`
- Priority: `P1`
- Why:
  - Engine currently focuses on exact-input execution; exact-output path is required for fuller V3 parity and routing flexibility.
- Deliverables:
  - Add exact-output swap step logic and execution/query support.
  - Maintain consistent fee and rounding behavior.
- Tasks:
  - [ ] Implement `compute_swap_step_exact_output` and integration path.
  - [ ] Add query/execute support where required.
  - [ ] Ensure multihop compatibility.
- Tests:
  - [ ] Math fixtures for exact-output step parity.
  - [ ] Integration tests for exact-output route simulation/execution parity.
- Done Criteria:
  - [ ] Exact-output concentrated swaps pass deterministic vectors and route tests.

### P1-006: Position token metadata enrichment (optional but recommended)
- Status: `Backlog`
- Priority: `P1`
- Why:
  - Position token currently mints with empty `token_uri`, limiting UX and observability.
- Deliverables:
  - Add position metadata encoding strategy (pool key, range, liquidity snapshots).
  - Optional metadata update on liquidity changes.
- Tasks:
  - [ ] Define metadata schema.
  - [ ] Populate metadata on mint.
  - [ ] Decide immutable vs mutable metadata policy.
- Tests:
  - [ ] Integration checks for metadata correctness.
- Done Criteria:
  - [ ] Position NFTs expose usable metadata for indexers/frontends.

### P2-007: Oracle hardening and parity checks
- Status: `Backlog`
- Priority: `P2`
- Why:
  - Observation system exists; needs deeper parity and edge-case validation.
- Deliverables:
  - Harden interpolation and cardinality growth behavior.
  - Add additional determinism checks.
- Tasks:
  - [ ] Add edge-case tests for stale/newest observation boundaries.
  - [ ] Validate behavior with zero-liquidity intervals and sparse updates.
  - [ ] Add TWAP consistency assertions over controlled swap timelines.
- Tests:
  - [ ] Unit oracle interpolation vectors.
  - [ ] Integration TWAP scenarios.
- Done Criteria:
  - [ ] Observe responses are stable and consistent across all tested windows.

## Current Sprint Proposal
- Sprint Goal:
  - Close critical CLP production gaps for correctness and operability.
- In Scope:
  - `P0-001`, `P0-002`, `P0-003`
- Stretch:
  - `P1-004` design + fixture scaffolding
- Out of Scope:
  - `P1-005`, `P2-007` full completion

## Verification Checklist
- [x] `cargo test -p concentrated_vlp --lib --tests -- --nocapture`
- [x] `cargo test -p tests-integration concentrated_v3_swap -- --nocapture`
- [x] `cargo test -p tests-integration concentrated_v3_fees -- --nocapture`
- [x] `cargo test -p tests-integration concentrated_v3_oracle -- --nocapture`
- [x] `cargo test -p tests-integration factory_swap_mixed_concentrated -- --nocapture`
- [ ] `cargo test -p tests-integration -- --nocapture`

## Notes
- Keep CP/Stable behavior unchanged.
- Keep `NextSwapPair.pool_key` explicit CLP semantics unchanged.
- Preserve position ID namespace scheme and ownership continuity through migration.
