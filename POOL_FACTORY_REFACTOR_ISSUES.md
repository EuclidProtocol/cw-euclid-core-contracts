# Pool Factory Refactor — Issues

Tracer-bullet vertical slices for SC-4 (Pool Factory Refactor). Each slice cuts end-to-end through main Factory stub → escrow → pool_factory handler → outbound → ProxySendPacket → IBC → OnPoolAck → downstream proxy (mint/burn/release).

Parent: [SC-4 — Pools Function Refactor](https://linear.app/euclid-protocol/issue/SC-4/pools-function-refactor)
Source plan: [POOL_FACTORY_REFACTOR.md](./POOL_FACTORY_REFACTOR.md)

## Dependency graph

```
Slice 1 (scaffold + CP create)
├── Slice 2 (CP add_liquidity)
├── Slice 3 (CP remove_liquidity)
└── Slice 4 (CLP create)
    ├── Slice 5 (CLP add)
    │   └── Slice 7 (CLP remove)
    └── Slice 6 (CLP collect fees)
            ↓
       Slice 8 (drain-and-cut migration) [HITL]
            ↓
       Slice 9 (query surface + schemas + docs)
```

All slices are AFK except Slice 8.

## Status

| Slice | Status | Notes |
|------:|:-------|:------|
| 1 | ✅ Done | Landed on `pools-functions-refactor` in commits `69d2740c` (scaffold + delegation + unit tests + CHANGELOG) and `04b2c7e8` (integration tests). 127 factory unit + 4 pool_factory unit + 2,475 integration tests pass. |
| 2 | ⬜ Not started | Unblocked. |
| 3 | ⬜ Not started | Unblocked. |
| 4 | ⬜ Not started | Unblocked. |
| 5 | ⬜ Blocked by Slice 4 | |
| 6 | ⬜ Blocked by Slice 4 | |
| 7 | ⬜ Blocked by Slices 4, 5 | |
| 8 | ⬜ HITL — Blocked by Slices 1–7 | `MigrateAcceptPoolState` stub already lives on pool_factory; full drain-and-cut runbook still to write. |
| 9 | ⬜ Blocked by Slice 8 | |

---

## Slice 1 — pool_factory scaffold + CP pool creation tracer

**Type:** AFK
**Status:** ✅ Done
**Blocked by:** None — can start immediately.

### What to build

First tracer-bullet slice for the chain-side pool factory split. Stand up the new `pool_factory` contract end-to-end and prove the cross-contract architecture by routing a single pool op — CP/Stable `RequestPoolCreation` — through the full pipeline:

- User calls main Factory `RequestPoolCreation` (surface unchanged).
- Main Factory generates `tx_id` and delegates to `pool_factory::on_request_pool_creation` via `WasmMsg::Execute`.
- Pool factory builds the outbound `RouterCrossChainExecuteMsg::RequestPoolCreation` binary via its `outbound` builder module and calls back into main Factory through `ProxySendPacket`.
- Main Factory's `ProxySendPacket` auth-checks `info.sender == POOL_FACTORY_ADDRESS`, then runs the existing `execute_send_packet` flow (sequence numbering, rate limiting, IBC dispatch). Native mode falls through the same path because `execute_send_packet` already branches on `is_native`.
- Ack arrives at main Factory's `execute_receive_acknowledgement`; the pool variants of `reusable_internal_ack_call` are removed and replaced with a forward to `pool_factory::on_pool_ack { original_msg, ack }`.
- Pool factory's `OnPoolAck` dispatcher matches the original variant and runs the `RequestPoolCreation` success/failure path. VLP/LP instantiate replies are owned by pool factory (its own reply ID namespace).

Establishes infrastructure every later slice inherits:

- New contract at `contracts/liquidity/pool_factory/` following the project structure (`contract.rs`, `execute/`, `query.rs`, `state.rs`, `reply.rs`, `interface.rs`, `mock.rs`, `migrate.rs`, `src/testing/`).
- Pool factory state items committed up-front (even if some are unused this slice): `MAIN_FACTORY_ADDRESS`, `MIGRATION_ACCEPTED`, `PAIR_TO_VLP`, `VLP_TO_LP_TOKEN`.
- Main Factory state items: `POOL_FACTORY_ADDRESS: Item<Addr>`, `POOL_FACTORY_INITIALISED` one-shot flag.
- Main Factory query: `QueryAdminRole { addr, role }` returning boolean (pool factory uses this for admin-gated actions instead of its own `EuclidAdmin`).
- Main Factory query: `QueryPoolFactoryAddress` for convenience/indexer use.
- Pool factory queries the matching ones for this slice: pool address by pair, LP token by VLP.
- Deploy helper in `tests-integration/helpers/` named `setup_factory_with_pool_factory` that instantiates both contracts and wires `POOL_FACTORY_ADDRESS` / `MAIN_FACTORY_ADDRESS`. Existing helpers continue to work for non-pool flows.
- Integration test: round-trip CP pool creation in all three chain modes (Native/IBC/EVM) via the rstest parameterisation pattern.
- Unit tests for `ProxySendPacket` (authorised + unauthorised caller), for the `outbound` builder (table-driven over the `RequestPoolCreation` variant inputs), and for pool factory's `on_request_pool_creation` handler (happy path + unauthorised caller).

Out of scope for this slice: add/remove liquidity flows, all CLP variants, migration. Those come in follow-up slices and re-use the same proxy + ack pattern.

### Acceptance criteria

- [x] `contracts/liquidity/pool_factory/` exists with the project's standard layout and compiles to wasm via `cargo wasm`.
- [x] Main Factory has `POOL_FACTORY_ADDRESS`, `POOL_FACTORY_INITIALISED`, `ProxySendPacket`, `QueryAdminRole`, `QueryPoolFactoryAddress`.
- [x] Pool factory has `MAIN_FACTORY_ADDRESS`, `on_request_pool_creation`, `outbound::request_pool_creation`, `on_pool_ack` dispatcher (with only the `RequestPoolCreation` arm wired this slice), VLP+LP instantiate reply IDs in its own namespace.
- [x] Main Factory's pool-handler stub for `RequestPoolCreation` delegates to pool factory; the old in-Factory code path is removed only for this variant (other variants remain untouched). *(Implementation note: rather than removing the legacy code path, it is gated on `POOL_FACTORY_INITIALISED`. Pre-bootstrap chains continue to use the in-Factory path; once `SetPoolFactory` or the Slice 8 migration flips the flag, the delegation path takes over. The gate will be removed in a follow-up Sirius release per the original plan.)*
- [x] `reusable_internal_ack_call` on main Factory forwards `RequestPoolCreation` ack to pool factory; other variants unchanged.
- [x] Integration test `pool_factory_cp_create` (or equivalent name) passes for Native, IBC, and EVM modes.
- [x] Unit tests: `ProxySendPacket` rejects non-pool-factory callers; `outbound` builder produces the expected serialised packet; `on_request_pool_creation` rejects non-main-Factory callers.
- [x] `cargo fmt --all -- --check`, `cargo clippy -- -W clippy::pedantic`, and `cargo unit-test --locked` all pass. *(Pedantic emits informational warnings; the existing factory has ~430 such warnings, pool_factory adds ~34. No errors at default clippy level.)*
- [x] `CHANGELOG.md` updated with `[factory]` and new `[pool_factory]` entries under the current Sirius in-progress section.

### Implementation notes carried into later slices

- `pool_factory::ExecuteMsg::MigrateAcceptPoolState` is already scaffolded (Slice 1 narrow payload: `pair_to_vlp`, `vlp_to_lp_token`). Slice 8 will extend the payload with `position_token_contract`, `concentrated_vlps`, `clp_position_id_vlp_map`, and the pending-queue items.
- `SetPoolFactory` admin entry on main Factory is the fresh-chain bootstrap path. Migration admin only, one-shot.
- `proxy.rs` deep module exports helpers used by later slices: `pool_factory_is_initialised`, `pool_factory_on_pool_ack_submsg`, `pool_factory_execute_msg`. Re-use these from Slices 2–7 instead of re-rolling delegation logic.
- `outbound.rs` is the home for all per-pool-op packet builders. Slices 2–7 add one builder each, all table-driven against `RouterCrossChainExecuteMsg`.
- The forward in `reusable_internal_ack_call` short-circuits on `is_pool_variant(&msg)`. Each later slice extends that matcher to include the additional pool variants it owns.

---

## Slice 2 — CP/Stable add_liquidity through pool_factory

**Type:** AFK
**Blocked by:** Slice 1

### What to build

Route CP/Stable `AddLiquidity` end-to-end through the pool factory pipeline established in Slice 1. Reuses the proxy + outbound + ack patterns from that slice.

Flow:

- Main Factory's `AddLiquidity` handler becomes a thin stub: validate inputs, generate `tx_id`, deposit funds to escrow via `SubMsg::reply_on_success(escrow_deposit, ESCROW_DEPOSIT_REPLY_ID)`.
- Escrow-deposit reply handler issues a `WasmMsg::Execute` to `pool_factory::on_add_liquidity { sender, pair, slippage_tolerance_bps, tx_id, … }` as a `SubMsg::reply_on_error` so escrow rollback is handled if pool factory rejects.
- Pool factory verifies caller is `MAIN_FACTORY_ADDRESS`, writes `PENDING_ADD_LIQUIDITY` keyed by `tx_id`, builds the `RouterCrossChainExecuteMsg::AddLiquidity` binary via `outbound`, calls `main_factory::proxy_send_packet`.
- Ack arrives at main Factory, forwarded to `pool_factory::on_pool_ack`. The `AddLiquidity` arm of the dispatcher is wired this slice. On success the ack handler emits `WasmMsg::Execute` to `main_factory::proxy_mint_lp_token { lp_token, to, amount }`. On failure the ack handler emits `proxy_release_escrow` to refund the user.
- Main Factory's `ProxyMintLpToken` auth-checks `info.sender == POOL_FACTORY_ADDRESS` then issues the cw20 mint as it does today. Main Factory remains the cw20 minter — no token-contract migration.

State items moved to pool factory in this slice: `PENDING_ADD_LIQUIDITY`, `FUNDS_INFO` (if not already moved in Slice 1). The deposit-to-escrow path on main Factory remains the same — escrow custody stays on main Factory.

User-facing semantics are unchanged: funds land in escrow before any pool-state mutation; failure paths refund through the same escrow surface.

### Acceptance criteria

- [ ] Main Factory's `AddLiquidity` handler is reduced to a thin stub + escrow-deposit submsg + reply-driven delegation to pool factory.
- [ ] Pool factory has `on_add_liquidity` handler, `outbound::add_liquidity` builder, `AddLiquidity` arm wired in `on_pool_ack`, `PENDING_ADD_LIQUIDITY` storage.
- [ ] Main Factory has `ProxyMintLpToken` with auth (`info.sender == POOL_FACTORY_ADDRESS`).
- [ ] `ProxyReleaseEscrow` is added to main Factory (auth-gated) and used by the ack failure path.
- [ ] Integration test `pool_factory_cp_add_liquidity` runs end-to-end in all three chain modes; on success the user's LP balance increases and escrow balances update; on failure (e.g. slippage rejection) the user gets a refund and no LP is minted.
- [ ] Unit tests: `on_add_liquidity` rejects non-main-Factory callers; `outbound::add_liquidity` is table-driven over inputs; `ProxyMintLpToken` and `ProxyReleaseEscrow` each have unauthorised-caller tests.
- [ ] Events and tx attributes (`tx_id`, `TxType`, `simple_event`, `tx_event`) emitted by the add-liquidity flow are unchanged from today (indexer compatibility).
- [ ] `cargo fmt --all -- --check`, `cargo clippy -- -W clippy::pedantic`, and `cargo unit-test --locked` all pass.
- [ ] `CHANGELOG.md` updated.

---

## Slice 3 — CP/Stable remove_liquidity through pool_factory

**Type:** AFK
**Blocked by:** Slice 1

### What to build

Route CP/Stable `RemoveLiquidity` end-to-end through pool factory. The LP burn must happen only after the hub-side ack confirms the burn target, so that a user cannot lose LP tokens without receiving the underlying.

Flow:

- Main Factory `RemoveLiquidity` handler becomes a thin stub: validate inputs, generate `tx_id`, transfer the LP tokens into a holding state owned by main Factory (existing cw20 escrow path), delegate to pool factory.
- Pool factory writes `PENDING_REMOVE_LIQUIDITY` keyed by `tx_id`, builds outbound packet, calls `ProxySendPacket`.
- Ack arrives, forwarded to `pool_factory::on_pool_ack`. The `RemoveLiquidity` arm is wired this slice. On success the ack handler issues:
  - `proxy_burn_lp_token { lp_token, from, amount }` to burn the LP tokens main Factory was holding.
  - `proxy_release_escrow { token, to, amount }` (one per asset of the pair) to release the underlying to the user.
- On failure, the held LP tokens are returned to the user — no burn happens.

Main Factory's `ProxyBurnLpToken` is auth-gated and wraps the existing cw20 burn path.

State items moved to pool factory in this slice: `PENDING_REMOVE_LIQUIDITY`.

### Acceptance criteria

- [ ] Main Factory's `RemoveLiquidity` handler is a thin stub that holds the LP tokens and delegates to pool factory.
- [ ] Pool factory has `on_remove_liquidity` handler, `outbound::remove_liquidity`, `RemoveLiquidity` arm in `on_pool_ack`, `PENDING_REMOVE_LIQUIDITY` storage.
- [ ] Main Factory has `ProxyBurnLpToken` with auth.
- [ ] Integration test `pool_factory_cp_remove_liquidity` runs in all three chain modes. Success path: LP balance decreases, underlying tokens land in user wallet, escrow balances decrease. Failure path: user's LP tokens are returned, no burn, no escrow release.
- [ ] Unit tests: `on_remove_liquidity` rejects non-main-Factory callers; `ProxyBurnLpToken` rejects non-pool-factory callers; ack-failure path returns LP to the user.
- [ ] Event/tx-attribute parity verified against pre-refactor remove-liquidity flow.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## Slice 4 — CLP pool creation through pool_factory

**Type:** AFK
**Blocked by:** Slice 1

### What to build

Route CLP `RequestConcentratedPoolCreation` end-to-end through pool factory, including the singleton position-token NFT contract. The position token remains one NFT contract per chain for all CLP positions, with main Factory as NFT admin — no token-contract migration.

Flow:

- Main Factory `RequestConcentratedPoolCreation` handler becomes a thin stub that validates input, generates `tx_id`, and delegates to `pool_factory::on_request_concentrated_pool_creation`.
- Pool factory writes any `PENDING_CONCENTRATED_*` entry needed for this op keyed by `tx_id`, builds outbound packet, calls `ProxySendPacket`.
- Ack handling: the `RequestConcentratedPoolCreation` arm of `on_pool_ack` is wired this slice. On success, pool factory writes `CONCENTRATED_VLPS` and may trigger position-token NFT instantiation (first CLP pool on a chain). Instantiate reply is owned by pool factory; the resulting `POSITION_TOKEN_CONTRACT` is stored in pool factory state. The NFT contract's admin remains main Factory — pool factory passes that through during instantiate.
- Add `ProxyMintPosition` to main Factory (auth-gated) — unused this slice, but added now with unauthorised-caller tests so the trust boundary is complete for follow-up CLP slices.

This slice introduces the `execute::clp` module on pool factory; existing `execute::cp` from prior slices is untouched.

State items moved this slice: `CONCENTRATED_VLPS`, `POSITION_TOKEN_CONTRACT`, `PENDING_CONCENTRATED_*` (the creation-related subset), `CONCENTRATED_FUNDS_INFO` if needed.

### Acceptance criteria

- [ ] Pool factory has `execute::clp` module with `on_request_concentrated_pool_creation` and pool-factory-owned reply IDs for VLP and (singleton) position-token NFT instantiate.
- [ ] Pool factory stores `CONCENTRATED_VLPS`, `POSITION_TOKEN_CONTRACT`, and the relevant `PENDING_CONCENTRATED_*` map.
- [ ] Main Factory's `RequestConcentratedPoolCreation` handler is a thin delegation stub.
- [ ] `on_pool_ack` dispatches the `RequestConcentratedPoolCreation` arm to the CLP module.
- [ ] Main Factory has `ProxyMintPosition` with auth (`info.sender == POOL_FACTORY_ADDRESS`) and a unauthorised-caller unit test.
- [ ] Position-token NFT contract is instantiated once per chain (singleton). Subsequent CLP pools reuse the existing NFT contract address. Admin of the NFT contract is main Factory.
- [ ] Integration test `pool_factory_clp_create` passes in all three chain modes.
- [ ] Unit tests: `on_request_concentrated_pool_creation` rejects non-main-Factory callers; `outbound::request_concentrated_pool_creation` table-driven test; `ProxyMintPosition` rejects non-pool-factory callers.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## Slice 5 — CLP add_concentrated_liquidity through pool_factory

**Type:** AFK
**Blocked by:** Slice 4

### What to build

Route CLP `AddConcentratedLiquidity` end-to-end through pool factory. Funds land in escrow on main Factory before any pool mutation; on ack success the singleton position-token NFT is minted (new position) or updated (existing position).

Flow:

- Main Factory `AddConcentratedLiquidity` becomes a thin stub: validate, generate `tx_id`, deposit funds to escrow via reply-on-success, then in the reply emit `WasmMsg::Execute` to `pool_factory::on_add_concentrated_liquidity`.
- Pool factory verifies caller is main Factory, writes `PENDING_CONCENTRATED_*` keyed by `tx_id`, builds outbound packet via `outbound::add_concentrated_liquidity`, calls `ProxySendPacket`.
- Ack arrives, forwarded to `pool_factory::on_pool_ack`. The `AddConcentratedLiquidity` arm is wired this slice. On success:
  - New position → `ProxyMintPosition { to, position_data }` on main Factory.
  - Existing position → `ProxyUpdatePosition { token_id, position_data }` on main Factory.
  - In either case, write `CLP_POSITION_ID_VLP_MAP` linking position ID to VLP.
- On failure, `ProxyReleaseEscrow` refunds the user.
- Add `ProxyUpdatePosition` to main Factory (auth-gated).

State moved/touched this slice: `CLP_POSITION_ID_VLP_MAP`, `PENDING_CONCENTRATED_*` add-side entries.

### Acceptance criteria

- [ ] Pool factory has `on_add_concentrated_liquidity` handler, `outbound::add_concentrated_liquidity`, `AddConcentratedLiquidity` arm in `on_pool_ack`.
- [ ] Main Factory has `ProxyUpdatePosition` with auth and a unauthorised-caller unit test.
- [ ] Main Factory `AddConcentratedLiquidity` handler is a thin stub + escrow-deposit submsg + reply-driven delegation.
- [ ] `CLP_POSITION_ID_VLP_MAP` writes happen in pool factory's ack handler.
- [ ] Integration test `pool_factory_clp_add_concentrated_liquidity` covers all three chain modes for both new-position and existing-position flows. Success path: position NFT minted/updated, escrow balances updated. Failure path: escrow refund, no position change.
- [ ] Unit tests: handler rejects non-main-Factory callers; outbound builder is table-driven; `ProxyUpdatePosition` auth tests; tick alignment, fee tier validation, and position lookup-miss edge cases covered.
- [ ] Event/tx-attribute parity verified.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## Slice 6 — CLP collect_concentrated_fees + protocol fees through pool_factory

**Type:** AFK
**Blocked by:** Slice 4

### What to build

Route both CLP `CollectConcentratedFees` (user) and `CollectConcentratedProtocolFees` (admin) end-to-end through pool factory. Independent of remove-liquidity; both depend only on Slice 4. Order of operations — request → hub ack → escrow release — is preserved identically to today.

Flow:

- Main Factory user-facing handlers become thin stubs: validate, generate `tx_id`, delegate to pool factory's `on_collect_concentrated_fees` / `on_collect_concentrated_protocol_fees`.
- Protocol-fee admin-gated path: pool factory queries `main_factory::query_admin_role { addr, role: FeeAdmin }` before proceeding — no `EuclidAdmin` on pool factory.
- Pool factory writes the relevant `PENDING_CONCENTRATED_*` entry, builds outbound packet, calls `ProxySendPacket`.
- Ack arrives, forwarded to `pool_factory::on_pool_ack`. The `CollectConcentratedFees` and `CollectConcentratedProtocolFees` arms are wired this slice. On success the ack handler issues `proxy_release_escrow` for each fee asset.
- On failure: no escrow release, no state change.

Position-token state is not mutated by fee collection in this slice — fee accounting changes happen on the hub side and arrive via the position-state query.

### Acceptance criteria

- [ ] Pool factory has `on_collect_concentrated_fees`, `on_collect_concentrated_protocol_fees`, matching outbound builders, and both arms wired in `on_pool_ack`.
- [ ] Protocol-fee admin gate uses `main_factory::QueryAdminRole` and rejects non-fee-admin callers.
- [ ] Integration tests `pool_factory_clp_collect_fees` and `pool_factory_clp_collect_protocol_fees` in all three chain modes. Success path: fee assets land in user/admin wallet through escrow release; pool state untouched. Failure path: no release, no state change.
- [ ] Unit tests: both handlers reject non-main-Factory callers; protocol-fee handler additionally rejects non-fee-admin originators; outbound builders are table-driven.
- [ ] Event/tx-attribute parity with pre-refactor fee collection.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## Slice 7 — CLP remove_concentrated_liquidity through pool_factory

**Type:** AFK
**Blocked by:** Slice 4, Slice 5

### What to build

Route CLP `RemoveConcentratedLiquidity` end-to-end through pool factory. The position-token NFT is burned or updated only after the hub-side ack confirms the burn target, mirroring the LP-token safety property in Slice 3.

Flow:

- Main Factory `RemoveConcentratedLiquidity` becomes a thin stub: validate, generate `tx_id`, delegate to `pool_factory::on_remove_concentrated_liquidity`.
- Pool factory writes a `PENDING_CONCENTRATED_*` remove-side entry keyed by `tx_id`, builds outbound packet, calls `ProxySendPacket`.
- Ack arrives, forwarded to `pool_factory::on_pool_ack`. The `RemoveConcentratedLiquidity` arm is wired this slice. On success:
  - Full withdrawal → `ProxyBurnPosition { token_id }` on main Factory; remove the `CLP_POSITION_ID_VLP_MAP` entry.
  - Partial withdrawal → `ProxyUpdatePosition { token_id, new_position_data }`.
  - Always → `ProxyReleaseEscrow` for each underlying asset returned.
- On failure: no NFT mutation, no escrow release.
- Add `ProxyBurnPosition` to main Factory (auth-gated).

### Acceptance criteria

- [ ] Pool factory has `on_remove_concentrated_liquidity` handler, `outbound::remove_concentrated_liquidity`, `RemoveConcentratedLiquidity` arm in `on_pool_ack`.
- [ ] Main Factory has `ProxyBurnPosition` with auth and an unauthorised-caller unit test.
- [ ] Full-withdrawal path: position NFT burned, `CLP_POSITION_ID_VLP_MAP` entry removed, escrow released.
- [ ] Partial-withdrawal path: position updated, no burn, escrow released.
- [ ] Failure path: position untouched, escrow untouched.
- [ ] Integration test `pool_factory_clp_remove_concentrated_liquidity` covers all three chain modes for full and partial withdrawal.
- [ ] Unit tests: handler rejects non-main-Factory callers; `ProxyBurnPosition` rejects non-pool-factory callers; ack-failure path leaves position intact.
- [ ] Event/tx-attribute parity.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## Slice 8 — Drain-and-cut migration: lock → drain → MigrateAcceptPoolState → unlock

**Type:** HITL
**Blocked by:** Slices 1–7

### What to build

Per-chain one-shot drain-and-cut migration that moves pool state from main Factory to pool factory. Marked HITL: operator runbook + migration safety review required before each chain rollout. Ships in the Sirius release alongside other Sirius migrations; each chain migrates independently.

Main Factory's Sirius-cycle migrate entry gains a `MigrateMsg` variant that carries `pool_factory_address`. Migration sequence (all in one transaction):

1. Precondition checks:
   - `STATE.locked == true` (chain locked via the existing `STATE.locked` mechanism — no new pause primitive).
   - All pool `PENDING_*` queues on main Factory empty.
   - `POOL_FACTORY_INITIALISED == false`.
2. Issue `WasmMsg::Execute { MigrateAcceptPoolState { pair_to_vlp, vlp_to_lp_token, position_token_contract, concentrated_vlps, clp_position_id_vlp_map, … } }` to pool factory. Payload is the full state snapshot from current storage.
3. Pool factory writes all incoming state, sets `MIGRATION_ACCEPTED = true`, sets `MAIN_FACTORY_ADDRESS`. Rejects if `MIGRATION_ACCEPTED` is already true or sender is not main Factory.
4. Main Factory nullifies (removes) the moved state items.
5. Main Factory writes `POOL_FACTORY_ADDRESS` and sets `POOL_FACTORY_INITIALISED = true`.

Atomicity: any error reverts the whole tx; chain remains locked; operator investigates and retries.

Main Factory's pool-handler stubs gate on `POOL_FACTORY_INITIALISED`: pre-flag → old in-Factory code path; post-flag → delegate. By this slice all variants are routed through pool factory, so the gate exists only to bridge the migration window. The gate is removed in a follow-up Sirius release.

LP token cw20 contracts and the singleton position-token NFT retain main Factory as cw20 minter / NFT admin — no token-contract migrations.

Bootstrap for new chains (post-refactor):

1. Deploy main Factory.
2. Deploy pool factory with `MAIN_FACTORY_ADDRESS` in instantiate msg.
3. Call main Factory's `SetPoolFactory` admin entry (new) with `pool_factory_address`, which writes `POOL_FACTORY_ADDRESS` and sets `POOL_FACTORY_INITIALISED = true`.

### Acceptance criteria

- [ ] Main Factory migrate entry validates locked-state, empty `PENDING_*`, and not-already-initialised preconditions; rejects with explicit errors per failure mode.
- [ ] Pool factory `MigrateAcceptPoolState` is one-shot, auth-gated to main Factory, sets `MIGRATION_ACCEPTED` on success.
- [ ] Migration atomically copies state and nullifies main Factory's copies; on any error the whole tx reverts and chain stays locked.
- [ ] Main Factory pool stubs gate on `POOL_FACTORY_INITIALISED`. Pre-flag handlers retain the old code path; post-flag handlers delegate.
- [ ] New chain bootstrap path: `SetPoolFactory` admin entry on main Factory, auth-gated to migration admin, idempotent only in the "not yet set" direction.
- [ ] Unit tests on main Factory: migrate fails when chain not locked; fails with non-empty `PENDING_*`; fails on second invocation; succeeds and emits the expected pool-factory call + state nullification.
- [ ] Unit tests on pool factory: `MigrateAcceptPoolState` rejects non-main-Factory callers; rejects second invocation; accepts and writes state correctly.
- [ ] Integration test: deploy pre-refactor-shape factory, seed pool state, lock chain, run migration, unlock, exercise a CP add-liquidity and a CLP add-liquidity round-trip post-migration in all three chain modes.
- [ ] `MIGRATION.md` updated with the drain-and-cut runbook (lock → drain → instantiate pool factory → run migrate → unlock).
- [ ] `CHANGELOG.md` updated with the migration entry under the current Sirius in-progress section.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.

---

## Slice 9 — Pool query surface, schemas, and migration docs

**Type:** AFK
**Blocked by:** Slice 8

### What to build

Final cleanup slice: complete the pool-query surface on pool factory, decide forwarding vs removal of main Factory's pre-existing pool queries, regenerate schemas, run cw-orch interface checks, and finish documentation.

Pool factory queries to expose (replacing the equivalent main-Factory queries that previously served them):

- Pool address by `Pair`.
- LP token by VLP address.
- Position-token contract address.
- Concentrated pool address by `PoolKey`.
- Position state (by token id).

Main Factory's pool-related queries: for each, either forward to pool factory (preserve the existing query address for indexers/frontends) or remove (if confirmed unused). The forward-vs-remove decision per query is driven by current frontend usage — gather usage signal from the frontend team and indexer ops before deciding. Default to "forward" when in doubt; removal is a follow-up.

`QueryPoolFactoryAddress` on main Factory (added in Slice 1) is verified end-to-end here.

Other deliverables:

- Regenerate JSON schema per contract (`cargo schema`).
- cw-orch interface tests: pool factory's `interface.rs` deploys correctly and exposes the typed call surface via `ExecuteFns` / `QueryFns`.
- `CHANGELOG.md`: final pass over the Sirius "in progress" section covering everything from this M1 (proxy-entry additions, pool-handler simplifications, new `[pool_factory]` section, migration notes).
- `MIGRATION.md`: any final edits after end-to-end migration testing in Slice 8.
- Confirm event names and tx attributes (`tx_id`, `TxType`, `simple_event`, `tx_event`) emitted by pool flows match pre-refactor, end-to-end through the integration suite.

### Acceptance criteria

- [ ] Pool factory exposes the full pool-query surface listed above. All queries have integration-test coverage.
- [ ] Each main-Factory pool query has an explicit forward-or-remove decision recorded in the PR description, with rationale (frontend usage signal cited).
- [ ] Forwarding queries return identical responses to the pre-refactor implementation (asserted in tests).
- [ ] `cargo schema` runs cleanly for both main Factory and pool factory; schema artifacts checked in.
- [ ] cw-orch interface for pool factory deploys correctly in tests-integration and the typed call surface compiles.
- [ ] Event/tx-attribute parity verified end-to-end against pre-refactor snapshots.
- [ ] `CHANGELOG.md` finalised for the Sirius release.
- [ ] `MIGRATION.md` finalised.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked`, `cargo wasm --locked` all pass.
