# Pool Factory Refactor (SC-4)

Linear: [SC-4 Pools Function Refactor](https://linear.app/euclid-protocol/issue/SC-4/pools-function-refactor)
Branch: `pools-functions-refactor`
Blocks: SC-5 (Router/Factory Size Reduction), SC-6 (External Audit)

## Problem Statement

The chain-side Factory contract has become a catch-all that owns IBC transport, user-facing entry points, escrow custody, denom/voucher state, and the entire pool lifecycle for three pool types (constant product, stable swap, concentrated liquidity). The CLP addition roughly doubled the pool-related code without restructuring the surrounding scaffolding — what was once a manageable "single reusable function" style of dispatch is now a 1000+ line `execute/pool.rs` and a large ack-side dispatcher in `ibc/ack_and_timeout.rs` that match over every `RouterCrossChainExecuteMsg` variant in sequence.

This shape has three concrete consequences:

1. **Bug-prone changes.** Pool changes touch unrelated code paths because pool, escrow, denom, and transport concerns are intermixed. CLP-specific changes risk affecting CP/Stable.
2. **Audit cost is too high.** SC-6 requires the chain-side Factory under audit. The current shape forces auditors to read the full contract to reason about a single pool flow.
3. **Blocks size reduction.** SC-5 wants to shrink router + Factory wasm bundles. The Factory cannot shrink while pool logic is fused into it.

## Solution

Split the chain-side Factory into two contracts deployed per chain:

- **Main Factory** keeps the user-facing ExecuteMsg surface, IBC port, sequence numbers, rate limiting, escrow registry, denom/voucher metadata, virtual-balance reference, and minter authority on every cw20 LP token and the singleton position-token NFT.
- A new **Pool Factory** contract owns pool registries (`PAIR_TO_VLP`, `VLP_TO_LP_TOKEN`, `POSITION_TOKEN_CONTRACT`, `CONCENTRATED_VLPS`, `CLP_POSITION_ID_VLP_MAP`), all pool `PENDING_*` queues, all pool-related reply IDs, and the per-pool-type business logic for creation, add/remove liquidity, and CLP fee collection. Per-pool-type isolation is at the module level (`execute/cp.rs`, `execute/clp.rs`) inside this single new contract.

Main Factory remains the only contract users and the relayer interact with. Pool Factory is an internal delegate: it is called by main Factory and only by main Factory, and it calls back into main Factory through narrow authorised proxy entries (`ProxySendPacket`, `ProxyMintLpToken`, `ProxyBurnLpToken`, `ProxyMintPosition`, `ProxyUpdatePosition`, `ProxyBurnPosition`, `ProxyReleaseEscrow`) to send IBC packets, mint/burn tokens, and release escrow on its behalf. Main Factory therefore retains custody and transport; pool factory holds logic and pool state.

Migration per chain is **drain-and-cut**: lock the chain via the existing `STATE.locked` mechanism, wait for in-flight `PENDING_*` entries to clear, instantiate pool factory, run a one-shot migrate flow that copies state and writes cross-references, unlock. Each chain migrates independently and the change rides the existing Sirius release migration train. The router and other hub-side contracts are not modified by SC-4; the matching hub-side refactor is SC-5's scope.

## User Stories

1. As a smart-contract auditor, I want pool lifecycle logic to live in a self-contained contract, so that I can audit pool flows without reading unrelated transport, escrow, and denom code.
2. As a smart-contract auditor, I want CLP-specific code in dedicated module files separate from CP/Stable code, so that I can confirm CLP behaviour without cross-referencing pool-agnostic helpers.
3. As a smart-contract auditor, I want the trust boundary between main Factory and pool factory expressed as a small set of authorised proxy entries, so that I can validate the auth checks in one read.
4. As a protocol developer, I want pool changes confined to one contract, so that CP/Stable bug fixes do not require re-deploying main Factory and vice versa.
5. As a protocol developer, I want a new pool type addable as a new module file rather than as additional branches in a giant match, so that future pool types do not regress the complexity that motivated this refactor.
6. As a protocol developer, I want pool factory's reply IDs in a separate namespace from main Factory's reply IDs, so that reply collisions are structurally impossible.
7. As a frontend developer, I want the user-facing ExecuteMsg surface on main Factory to remain identical, so that existing wallets, SDKs, and scripts do not need updates when the refactor ships.
8. As a CW20 token holder, I want my LP tokens to retain the same cw20 contract address and minter after the migration, so that wallets and approvals continue to work.
9. As a CLP position holder, I want my position NFTs to retain the same NFT contract address and admin after the migration, so that my positions remain accessible.
10. As a user creating a pool, I want pool creation to flow through main Factory exactly as before, so that fund custody and cw20 send hooks continue to land at the address I expect.
11. As a user adding liquidity, I want funds to be deposited to escrow before any pool-state mutation, so that escrow accounting and pool-state accounting cannot diverge if a step fails.
12. As a user removing liquidity, I want my LP token burn to happen only after the corresponding hub-side acknowledgement confirms the burn target, so that I cannot lose LP tokens without receiving the underlying.
13. As a user collecting CLP fees, I want the fee release to flow through main Factory's escrow path, so that fee payouts use the same custody surface as every other payout on the chain.
14. As a relayer operator, I want IBC channel topology to remain unchanged, so that no new channel handshake or port-binding configuration is required to support this refactor.
15. As a protocol operator, I want to migrate each chain on its own schedule, so that an outage on one chain does not delay rollout on others.
16. As a protocol operator, I want migration to be a one-shot operation per chain that cannot be repeated, so that accidental re-migration cannot corrupt state.
17. As a protocol operator, I want to lock and drain a chain before migration using the existing `STATE.locked` mechanism, so that no new pause primitive is introduced.
18. As a protocol operator, I want migration to fail atomically if any step errors, so that I never end up with half-migrated state.
19. As a general admin, I want admin status to be sourced from main Factory, so that I rotate admins once and the change applies to both contracts.
20. As a fee admin, I want fee parameters to remain on main Factory, so that fee changes do not require coordinating across two contracts.
21. As a migration admin, I want a single migration admin entry per contract per release, so that the migration surface is small and auditable.
22. As a CosmWasm developer, I want `tx_id` generation to remain monotonic and globally unique on each chain, so that pending queues and event traces remain consistent.
23. As a CosmWasm developer, I want pool factory to receive `tx_id` from main Factory rather than maintain its own counter, so that there is no risk of collision with non-pool tx ids.
24. As a CosmWasm developer, I want pool factory's outbound IBC packets to flow through main Factory's existing `execute_send_packet`, so that sequence numbering and rate limiting remain in one place.
25. As a CosmWasm developer, I want ack handling for pool messages to dispatch from main Factory's IBC entry to pool factory's `OnPoolAck` handler, so that the IBC port owner is unchanged.
26. As an integration test author, I want a single helper that deploys main Factory and pool factory together with the right cross-references, so that test setup does not duplicate wiring logic.
27. As an integration test author, I want existing pool-flow tests to keep their assertions and behaviour, so that the refactor does not silently change tested invariants.
28. As an integration test author, I want the existing Native/IBC/EVM rstest parameterisation to continue to apply to pool flows, so that all three chain modes are covered post-refactor.
29. As a unit test author, I want pool factory to expose its handlers for direct invocation in unit tests, so that per-pool-type edge cases can be table-driven without spinning up cw-multi-test.
30. As a unit test author, I want main Factory's proxy entries (`ProxySendPacket`, `ProxyMint*`, `ProxyBurn*`, `ProxyReleaseEscrow`) to have explicit unit tests covering both the authorised and unauthorised sender cases, so that the trust boundary is verified.
31. As a CLP user, I want the order of operations for fee collection (request → hub ack → escrow release) to behave identically to today, so that my fee payouts have the same semantics.
32. As a CLP user, I want my position-token contract to remain the singleton it is today (one NFT contract per chain for all CLP positions), so that token enumeration is unchanged.
33. As a hub-side developer, I want SC-4 to leave router and hub contracts unmodified, so that the matching hub-side refactor under SC-5 can be planned and reviewed independently.
34. As a backend indexer maintainer, I want event names and tx attributes (`tx_id`, `TxType`, `simple_event`, `tx_event`) emitted by pool flows to remain the same, so that downstream indexing is unaffected.
35. As a deployer, I want pool factory's wasm artifact built and uploaded once cluster-wide, so that per-chain deployment is reduced to a single instantiate + admin call.
36. As a deployer, I want main Factory's pre-existing instances to retain their addresses, so that on-chain bookmarks and contract references in other contracts continue to resolve.

## Implementation Decisions

### Contracts

**New: `pool_factory`.** Deployed per chain. Lives under `contracts/liquidity/pool_factory/` following the structure of other contracts (`contract.rs`, `execute/` directory, `query.rs`, `state.rs`, `reply.rs`, `interface.rs`, `mock.rs`, `migrate.rs`, `src/testing/`). Cw-orch interface and mock follow the pattern of existing liquidity contracts.

**Modified: `factory` (main Factory).** Existing user-facing ExecuteMsg variants unchanged. Pool-handler bodies shrink to thin stubs that prepare funds and delegate to pool factory. New proxy entries added. State items related to pools are removed during migration. Reply IDs for pool flows are removed.

### Module sketch

**Pool factory (new):**

- `state` — moved items: `PAIR_TO_VLP`, `VLP_TO_LP_TOKEN`, `POSITION_TOKEN_CONTRACT`, `CONCENTRATED_VLPS`, `CLP_POSITION_ID_VLP_MAP`, `PENDING_REMOVE_LIQUIDITY`, `PENDING_CONCENTRATED_*`, `FUNDS_INFO`, `CONCENTRATED_FUNDS_INFO`. New: `MAIN_FACTORY_ADDRESS`, a one-shot `MIGRATION_ACCEPTED` flag.
- `execute::cp` — CP/Stable pool ops: `on_request_pool_creation`, `on_add_liquidity`, `on_remove_liquidity`.
- `execute::clp` — CLP pool ops: `on_request_concentrated_pool_creation`, `on_add_concentrated_liquidity`, `on_remove_concentrated_liquidity`, `on_collect_concentrated_fees`, `on_collect_concentrated_protocol_fees`.
- `execute::ack` — `on_pool_ack` dispatcher that takes the original `RouterCrossChainExecuteMsg` plus ack and routes to per-pool-type ack handlers in `cp` and `clp` modules.
- `execute::migrate` — `migrate_accept_pool_state` (one-shot, auth via admin query to main Factory).
- `reply` — pool factory's own reply IDs (`VLP_INSTANTIATE`, `LP_INSTANTIATE`, `POSITION_TOKEN_INSTANTIATE`, `ADD_LIQUIDITY`, `REMOVE_LIQUIDITY`, `COLLECT_CONCENTRATED`).
- `admin` — `query_admin_role` helper that queries main Factory's `EuclidAdmin` and returns role membership for a caller.
- `outbound` — packet builder helpers: per-pool-op functions that build the matching `RouterCrossChainExecuteMsg` variant and return its serialised binary. Deep module candidate (pure logic, fixed interface, easily testable in isolation).

**Main Factory (modified):**

- `execute::proxy` (new) — `proxy_send_packet`, `proxy_mint_lp_token`, `proxy_burn_lp_token`, `proxy_mint_position`, `proxy_update_position`, `proxy_burn_position`, `proxy_release_escrow`. Each is one-line auth check (`info.sender == POOL_FACTORY_ADDRESS`) followed by forwarding to the matching existing or thin-wrapper functionality. Deep module candidate.
- `execute::pool` — handlers become thin: take funds, deposit to escrow as a `SubMsg::reply_on_success`, in the reply handler issue a `WasmMsg::Execute` to pool factory's matching `On*` entry with the prepared inputs including a freshly generated `tx_id`.
- `ibc::ack_and_timeout` — `reusable_internal_ack_call` retains non-pool variants (`RegisterDenom`, `DeregisterDenom`, `DepositToken`, `TransferVoucher`, `Swap`). Pool variants forward to pool factory via `WasmMsg::Execute { OnPoolAck { original_msg, ack } }`.
- `state` — removed: every state item listed in pool factory's `state` module above. Added: `POOL_FACTORY_ADDRESS: Item<Addr>`, and a one-shot `POOL_FACTORY_INITIALISED` flag.
- `reply` — pool-related reply IDs removed. Escrow-deposit reply id retained and gains an additional downstream action (issuing the `WasmMsg::Execute` to pool factory).
- `migrate` — Sirius-cycle migrate gains the pool-factory cut-over flow: validate `locked == true`, validate all pool `PENDING_*` queues are empty, push state copies via `WasmMsg::Execute { MigrateAcceptPoolState { … } }` to pool factory, nullify moved items, set `POOL_FACTORY_ADDRESS`, set `POOL_FACTORY_INITIALISED = true`. All in one tx.

### Call flow (user-initiated pool op, canonical example: add_liquidity)

1. User calls `factory::execute(AddLiquidity { … })` with attached funds (cw20 receive or native).
2. Main Factory validates inputs, generates next `tx_id` from its counter, looks up escrow address, emits `SubMsg::reply_on_success(escrow_deposit, ESCROW_DEPOSIT_REPLY_ID)`.
3. Escrow deposit succeeds; reply handler issues `WasmMsg::Execute` to `pool_factory::on_add_liquidity { sender, pair, slippage_tolerance_bps, tx_id, … }` as a SubMsg with `reply_on_error`.
4. Pool factory verifies caller is `main_factory_address`, writes its `PENDING_*` entry keyed by `tx_id`, builds the `RouterCrossChainExecuteMsg::AddLiquidity` binary via `outbound`, emits a `WasmMsg::Execute` to `main_factory::proxy_send_packet { binary, sender, ack_response }`.
5. Main Factory's `proxy_send_packet` verifies caller is `pool_factory_address`, then invokes the existing `execute_send_packet` flow which increments sequence, applies rate limiting, and emits the send-packet event.
6. (Later) IBC ack arrives at main Factory's `execute_receive_acknowledgement`. Original `RouterCrossChainExecuteMsg` is decoded. Since the variant is `AddLiquidity` (a pool variant), main Factory forwards via `WasmMsg::Execute` to `pool_factory::on_pool_ack { original_msg, ack }`.
7. Pool factory's ack handler matches on the original variant, runs the `AddLiquidity` success or failure path, and on success issues `WasmMsg::Execute` to `main_factory::proxy_mint_lp_token { lp_token, to, amount }` for the LP mint.
8. Main Factory's `proxy_mint_lp_token` verifies caller is `pool_factory_address`, then issues the cw20 mint as it does today.

The pattern is symmetric across all pool ops. Native chain mode goes through the same flow because `execute_send_packet` already branches on `is_native` internally — pool factory is agnostic to chain mode.

### Authorisation model

- Main Factory's pool-related ExecuteMsg variants: public (user-facing, unchanged from today).
- Main Factory's `Proxy*` entries: `info.sender == POOL_FACTORY_ADDRESS`.
- Pool factory's `On*` and `OnPoolAck` entries: `info.sender == MAIN_FACTORY_ADDRESS`.
- Pool factory's `MigrateAcceptPoolState`: `info.sender == MAIN_FACTORY_ADDRESS && !MIGRATION_ACCEPTED`. Sets `MIGRATION_ACCEPTED = true` on success; subsequent calls fail.
- Admin-gated actions on pool factory: pool factory queries `main_factory::query_admin_role { addr }` and proceeds only if the caller holds the relevant role. No `EuclidAdmin` storage on pool factory.

### `tx_id` generation

Main Factory remains the sole holder of the chain's `tx_id` counter. Pool factory never increments it. Main Factory generates the new `tx_id` in step 2 above and passes it to pool factory in the SubMsg payload. Pool factory uses it both as the `PENDING_*` key and as the `tx_id` field in the outbound `RouterCrossChainExecuteMsg`.

### Schema and API contracts

- `RouterCrossChainExecuteMsg` enum (in `packages/euclid_ibc`): unchanged. Pool factory uses existing variants verbatim when building outbound packets.
- `FactoryCrossChainExecuteMsg` enum: unchanged. Inbound `RegisterFactory` and `ReleaseEscrow` continue to land at main Factory.
- `factory::ExecuteMsg`: user-facing variants unchanged. New variants added: `ProxySendPacket`, `ProxyMintLpToken`, `ProxyBurnLpToken`, `ProxyMintPosition`, `ProxyUpdatePosition`, `ProxyBurnPosition`, `ProxyReleaseEscrow`. These are auth-gated but live in the same enum (CosmWasm idiom).
- `factory::QueryMsg`: gains `QueryAdminRole { addr, role }` returning a boolean for pool factory's admin checks. Optional: a `QueryPoolFactoryAddress` for convenience and indexer use.
- `pool_factory::ExecuteMsg`: `OnRequestPoolCreation`, `OnRequestConcentratedPoolCreation`, `OnAddLiquidity`, `OnAddConcentratedLiquidity`, `OnRemoveLiquidity`, `OnRemoveConcentratedLiquidity`, `OnCollectConcentratedFees`, `OnCollectConcentratedProtocolFees`, `OnPoolAck`, `MigrateAcceptPoolState`.
- `pool_factory::QueryMsg`: queries equivalent to today's pool-related queries on main Factory: pool address by pair, LP token by VLP, position token address, concentrated pool address by `PoolKey`, position state. Main Factory's existing pool-related queries either forward to pool factory or are removed (decision in follow-up — frontend's existing query addresses determine which).
- Events and tx attributes: unchanged. `simple_event`, `tx_event`, `TxType` values for pool flows preserved.

### State migration (per chain)

The migration is one transaction initiated by the migration admin against main Factory's migrate entry. Sequence:

1. Precondition checks: `STATE.locked == true`, all pool `PENDING_*` queues empty, `POOL_FACTORY_ADDRESS` provided in `MigrateMsg`, `POOL_FACTORY_INITIALISED == false`.
2. Issue a `WasmMsg::Execute { MigrateAcceptPoolState { pair_to_vlp, vlp_to_lp_token, position_token_contract, concentrated_vlps, clp_position_id_vlp_map, … } }` to pool factory. Payload is the full state snapshot built from current storage.
3. Pool factory writes all incoming state into its storage, sets `MIGRATION_ACCEPTED = true`, sets `MAIN_FACTORY_ADDRESS`. Returns.
4. Main Factory nullifies (removes) the moved state items.
5. Main Factory writes `POOL_FACTORY_ADDRESS` and sets `POOL_FACTORY_INITIALISED = true`.

If any step errors, the whole tx reverts; chain is still locked; operator can investigate and retry.

After unlock, main Factory's pool-handler stubs gate on `POOL_FACTORY_INITIALISED` — pre-migration chains continue to use the old code path until migrated. (This gate exists only during the migration window of the Sirius release; subsequent releases can remove it.)

LP token cw20 contracts and the position-token NFT contract retain main Factory as their cw20 minter / NFT admin. No token-contract migrations are required.

### Bootstrap (post-migration; for new chains)

For chains deployed after this refactor:

1. Deploy main Factory (existing instantiate flow).
2. Deploy pool factory with `MAIN_FACTORY_ADDRESS` in instantiate msg.
3. Call main Factory's migrate or an `SetPoolFactory` admin entry with `pool_factory_address`.
4. (Existing flow) Main Factory continues to instantiate the escrow contracts, position token NFT, and LP tokens on demand. Pool factory queries main Factory for these addresses or receives them on-demand via instantiate replies originating from main Factory's `Proxy*` entries.

### Hub-side scope

Router and hub contracts are untouched. The 1181-line `router/src/ibc/receive/pool.rs` and the dispatcher in `router/src/ibc/receive/base.rs` remain as they are today. The matching hub-side refactor (extract pool dispatch from router) is SC-5's scope.

### Deep modules identified

Two deep modules are extractable and worth testing in isolation:

- **Main Factory's `proxy` module.** Narrow interface (each proxy entry is auth-check + forward), few methods, rarely changes once shipped. The auth surface is the entire interface; the rest is delegation to existing functionality.
- **Pool factory's `outbound` module.** Pure builder functions: input is a typed pool-op request; output is a serialised `RouterCrossChainExecuteMsg` binary. No state, no side effects, no cross-contract calls. Easy table-driven tests.

A third candidate is **pool factory's `ack` dispatcher** — pure routing of `RouterCrossChainExecuteMsg` variants to per-pool-type handlers. The handlers themselves are not deep (they mutate state and emit submessages), but the dispatcher is.

## Testing Decisions

**What makes a good test here.** Tests assert external behaviour visible at contract boundaries: emitted events, emitted submessages and their payloads, returned errors, post-call state queryable through public queries. Tests do not assert internal storage layout, internal helper return shapes, or the specific sequence of internal helper calls. The trust boundary's auth checks are the one place where "this call from X must fail and from Y must succeed" is the behaviour under test.

**Existing prior art.**

- `tests-integration/` (cw-orch 0.28 + rstest parameterised across Native/IBC/EVM) — every pool flow is exercised in all three chain modes via shared helpers (`setup_router`, `setup_factory`, token / pool creation helpers). Pool-flow integration tests already exist for CP, Stable, and CLP creation, add/remove liquidity, fee collection.
- Per-contract `src/testing/` unit suites — each contract has a fixtures-driven rstest suite. The factory's existing pool unit tests in `factory/src/testing/` cover handler bodies directly via `MockDeps`.
- The unit-test-writer agent has been used previously to add table-driven coverage to individual contracts; the project conventions it follows are compatible with the new pool factory.

**Modules to test.**

- **Pool factory unit suite.** Full per-handler coverage for each `On*` entry: happy path, slippage rejection, invalid pair, unauthorised caller (must be main Factory), CLP-specific cases (tick alignment, fee tier validation, position lookup miss). Table-driven where the input space allows. Covers both `execute::cp` and `execute::clp` modules. Use the unit-test-writer agent for the initial generation.
- **Pool factory ack dispatcher.** Each `RouterCrossChainExecuteMsg` pool variant routed through `OnPoolAck` with both success and error acks. Asserts the right downstream message is emitted (the proxy call back to main Factory) and the right state mutation occurs.
- **Pool factory outbound module (deep module).** Pure table-driven unit tests over the builder functions: input request → expected serialised `RouterCrossChainExecuteMsg` binary. No `MockDeps` needed.
- **Main Factory proxy module (deep module).** Per proxy entry: caller is pool factory (auth passes, downstream message emitted as expected) and caller is anything else (returns `Unauthorized`). Small, repetitive, easy to keep complete.
- **Main Factory pool handler stubs.** Confirm each user-facing pool ExecuteMsg variant runs the funds/escrow path and emits the expected `WasmMsg::Execute` to pool factory with the right `tx_id` and payload. Stops at the boundary — does not assert what pool factory does.
- **Migration unit tests.** On main Factory: migrate fails when chain not locked, fails with non-empty `PENDING_*`, fails on second invocation, succeeds and produces the expected pool factory call + state nullification. On pool factory: `MigrateAcceptPoolState` rejects non-main-Factory callers, rejects second invocation, accepts and writes state correctly.
- **Integration suite (end-to-end, with both contracts deployed).** All existing pool-flow tests in `tests-integration/` continue to run in all three chain modes (Native, IBC, EVM) against a deployed main Factory + pool factory pair. A shared helper `setup_factory_with_pool_factory` wires the pair. Existing assertions are unchanged. New tests added: at least one round-trip per pool op type per chain mode confirming the cross-contract flow completes and yields the expected on-chain state (LP balance increase, position NFT minted, escrow updated).
- **Cw-orch interface tests.** Pool factory's `interface.rs` deploys correctly and exposes the typed call surface via `ExecuteFns` / `QueryFns`.

## Out of Scope

- Router (hub-side) pool dispatch refactor — that's SC-5.
- Any change to `RouterCrossChainExecuteMsg` or `FactoryCrossChainExecuteMsg` enum variants.
- Splitting pool factory into separate CP/Stable and CLP contracts (considered and rejected — single contract with per-pool-type modules).
- New IBC channels, ports, or relayer config changes.
- LP token / position token contract migrations (none required under M2).
- Cross-chain `Swap` refactor — swap stays on main Factory.
- Token / denom registration refactor — stays on main Factory.
- Voucher / virtual balance refactor — stays on main Factory.
- Independent admin rotation on pool factory — admin is queried from main Factory.
- Pool factory ever having its own `tx_id` counter — main Factory remains sole counter holder.
- Pre-Sirius release rollout — this ships in the Sirius release alongside its other migrations.

## Further Notes

- The "single reusable function" complaint in the task description is interpreted as referring to the per-contract dispatchers (`reusable_internal_call`, `reusable_internal_ack_call`) that match over every IBC message variant in sequence. After this refactor, the chain-side dispatcher on main Factory retains only non-pool variants; pool factory has its own smaller dispatcher over pool variants. Within pool factory, each pool type's handler lives in its own module file (`cp.rs`, `clp.rs`).
- Pool factory does not need to know about `is_native`, IBC channels, or relayer state. All transport-mode awareness stays in main Factory's `execute_send_packet` flow.
- `CHANGELOG.md` entry should land under the current Sirius "in progress" section once implementation lands. Per-contract entries: `[factory]` for the proxy-entry additions and pool-handler simplifications; new `[pool_factory]` section for the new contract.
- The `MIGRATION.md` doc should gain a section describing the per-chain drain-and-cut sequence (lock → drain → instantiate pool factory → run migrate → unlock).
- The cw-orch `ExecuteFns` and `QueryFns` derives on the new ExecuteMsg / QueryMsg enums ensure the existing typed-calling pattern works without test-helper changes beyond `setup_factory_with_pool_factory`.
