# Pool Factory Refactor — Reply-Data Send-Packet Auth (SC-4 Amendment)

Linear parent: [SC-4 Pools Function Refactor](https://linear.app/euclid-protocol/issue/SC-4/pools-function-refactor)
Branch: `pools-functions-refactor`
Supersedes (for `ProxySendPacket` only): [POOL_FACTORY_REFACTOR.md](./POOL_FACTORY_REFACTOR.md), [POOL_FACTORY_REFACTOR_ISSUES.md](./POOL_FACTORY_REFACTOR_ISSUES.md)

> This document is an **amendment** to the SC-4 plan. It replaces the architectural choice around `ProxySendPacket` only; every other decision in the original plan (per-pool-type modules, drain-and-cut migration, `OnPoolAck` dispatcher, `Proxy*` mint/burn/release entries, `tx_id` ownership on main Factory, etc.) is unchanged.

## Problem Statement

The SC-4 refactor introduces a `ProxySendPacket` ExecuteMsg variant on main Factory. Its job is to let pool factory hand main Factory a `RouterCrossChainExecuteMsg` binary so that main Factory can run `execute_send_packet` — the function that owns sequence numbering, rate limiting, IBC packet emission, and (for native chains) the direct-router callback fallback.

The entry is auth-gated to `info.sender == POOL_FACTORY_ADDRESS`. That single sender check is the entire trust boundary protecting an extremely powerful capability: **emit an arbitrary cross-chain message**. The binary payload can be any variant of `RouterCrossChainExecuteMsg` — pool ops, but also `Swap`, `TransferVoucher`, `RegisterDenom`, anything the router accepts. A bug in pool factory, a misrouted entry point, a future contract that mistakenly forwards on its behalf, or any defect that causes `info.sender` to equal `POOL_FACTORY_ADDRESS` under attacker control would let the attacker drive cross-chain swaps, voucher transfers, or denom mutations through main Factory's IBC port.

Concretely, three properties make this entry the highest-leverage attack surface added by SC-4:

1. **Public ExecuteMsg variant.** Any address on chain can dispatch to it. The auth check is the *only* gate.
2. **Arbitrary payload.** The `msg: Binary` field is not type-restricted to pool variants. The check is on the sender, not on what the sender is asking for.
3. **Direct access to the IBC port.** The function output is a real IBC packet (or a direct router execute), bypassing main Factory's user-level rate limiting, escrow accounting, and pool-flow gating.

By contrast, the other `Proxy*` entries (mint LP, burn LP, release escrow, mint/burn/update position) are scoped: each one performs a narrow, type-checked action on a specific resource. Misuse of those is bounded by the resource. Misuse of `ProxySendPacket` is bounded only by what the router will execute — i.e., by the whole router-side message surface.

The plain `ExecuteMsg` shape also makes the surface easy to *forget about*: every future enum expansion, every new caller, every refactor to the auth helpers must be re-audited against this entry. The contract design carries one persistent latent risk that does not shrink with time.

## Solution

Remove the `ProxySendPacket` ExecuteMsg variant from main Factory entirely. Replace it with a **reply-data return** flowing in the opposite direction: pool factory communicates the packet it wants sent by setting `Response::data` on the handler that main Factory dispatched it as a SubMsg, and main Factory's reply handler is the only thing that ever invokes `execute_send_packet`.

The flow inverts the call direction without changing what happens:

- **Before (current Slice 1–4 implementation):** main Factory → pool factory `On*` handler (fire-and-forget SubMsg) → pool factory → main Factory `ProxySendPacket` (fire-and-forget SubMsg) → `execute_send_packet`.
- **After:** main Factory → pool factory `On*` handler (`SubMsg::reply_on_success`) → pool factory returns `Response { data: Some(SendPacketRequest{…}), … }` → main Factory's reply handler decodes the data and calls `execute_send_packet` directly.

The data path is internal to a single transaction's reply chain. There is no public ExecuteMsg variant an attacker can call to trigger `execute_send_packet`. The only contract that can produce a "send-packet" reply payload is one that main Factory itself dispatched as a SubMsg, with a reply ID main Factory owns. Reaching the IBC port now requires *causing main Factory to dispatch your code as a submsg*, not just *being able to send it a message*.

This trades a sender-auth check for a structural one. Sender auth is a runtime predicate that can fail open under bugs; the structural property — "the reply only fires for submessages I initiated" — is a CosmWasm VM invariant.

We additionally keep a **defence-in-depth content check** in the reply handler: before calling `execute_send_packet`, validate that the decoded `RouterCrossChainExecuteMsg` is a pool variant (the same `is_pool_variant` matcher already used in the inbound ack dispatcher). A pool factory bug that returns a non-pool packet is rejected at the boundary instead of silently emitting a swap or voucher transfer.

The matching `Proxy*` entries for mint, burn, and escrow release are *not* changed by this PRD — their auth surface is bounded by resource type, and converting them costs more in complexity than it gains in safety. They remain auth-gated ExecuteMsg variants as designed. Further work to apply the same reply-data pattern to those entries is noted as future scope.

## User Stories

1. As a smart-contract auditor, I want there to be no public ExecuteMsg variant on main Factory that can emit an arbitrary IBC packet, so that I do not need to audit a sender-auth check whose failure mode is "any cross-chain message".
2. As a smart-contract auditor, I want pool factory's ability to drive the IBC port to derive from being a submsg main Factory itself dispatched, so that the trust boundary is structural (CosmWasm reply scoping) rather than a runtime sender check.
3. As a smart-contract auditor, I want main Factory's reply handler to assert that the decoded packet is a pool variant before sending, so that defence in depth protects against bugs in pool factory.
4. As a smart-contract auditor, I want the per-slice trust-boundary surface to shrink, not grow, with each slice that lands, so that the audit cost is monotonically decreasing across Slices 1–7.
5. As a protocol developer, I want pool factory to communicate "please send this packet" via a typed `Response::data` payload, so that the contract between the two factories is expressed as a typed message rather than as an out-of-band ExecuteMsg call.
6. As a protocol developer, I want a single reply ID on main Factory dedicated to pool-factory delegated calls, so that all pool-factory submsg replies route through one auditable code path.
7. As a protocol developer, I want pool factory's `outbound` builder module to remain the single source of truth for outbound packet construction, so that switching to reply-data does not duplicate packet-build logic.
8. As a protocol developer, I want the public ExecuteMsg surface on main Factory to be the same or smaller after this change, so that the schema diff is "removed `ProxySendPacket`" with no additions.
9. As a CosmWasm developer, I want pool factory's `On*` handlers to be dispatched with `reply_on_success`, so that any error inside pool factory reverts the whole tx (including the escrow deposit) and the user does not end up with funds in escrow and no pool action.
10. As a CosmWasm developer, I want pool factory to set `Response::data` only on success, so that the reply handler's data-presence check is the gate to running `execute_send_packet`.
11. As a CosmWasm developer, I want the data payload to be a typed enum (not a raw binary), so that adding new reply-driven actions later (e.g., reply-data variants for mint/burn) is a non-breaking enum extension.
12. As a CosmWasm developer, I want main Factory's reply handler to differentiate a "no data set" reply (treat as no-op / error) from a "data set" reply (decode and act), so that pool factory handlers that legitimately produce no outbound packet (e.g., bootstrap handlers, error short-circuits) still work.
13. As a relayer operator, I want the IBC packet emission semantics — packet contents, sequence numbering, timeout, ack response, native fallback — to be byte-identical before and after this change, so that no relayer reconfiguration is needed.
14. As a frontend developer, I want the user-facing ExecuteMsg surface on main Factory to remain unchanged, so that the schema diff is invisible to wallets and SDKs.
15. As a CW20 LP token holder, I want my LP tokens to retain their cw20 contract address and minter unchanged, so that wallets and approvals continue to work — this PRD does not touch the `Proxy*` mint/burn entries.
16. As a CLP position holder, I want my NFT contract and admin to remain unchanged, so that positions remain accessible — this PRD does not touch `ProxyMintPosition` / `ProxyUpdatePosition` / `ProxyBurnPosition`.
17. As a protocol operator, I want the migration sequence (lock → drain → `MigrateAcceptPoolState` → unlock) to be unchanged by this amendment, so that Slice 8's runbook is not invalidated.
18. As a protocol operator, I want each already-landed slice (1, 2, 3, 4) to be retrofitted in its own PR, so that the change can be reviewed slice-by-slice rather than as a single sweeping rewrite.
19. As an integration-test author, I want all existing `pool_factory_cp_*` and `pool_factory_clp_*` integration tests to keep passing without assertion changes, so that the behavioural contract is provably preserved.
20. As a unit-test author, I want a single new test surface — "given a synthetic reply with this data, main Factory's reply handler does X" — to replace the deleted `execute_proxy_send_packet` unit tests, so that the auth-boundary coverage moves from sender-check tests to data-decode tests.
21. As a unit-test author, I want a dedicated negative test confirming the reply handler rejects a non-pool `RouterCrossChainExecuteMsg` variant, so that the defence-in-depth content check is exercised.
22. As an indexer maintainer, I want emitted events and tx attributes for pool flows to be unchanged, so that downstream indexing is unaffected.
23. As a CHANGELOG reviewer, I want the change recorded as a `[factory]` security/improvement entry under the current Sirius in-progress section, with a brief note that `ProxySendPacket` was removed in favour of reply-data delegation.

## Implementation Decisions

### Contracts modified

**`factory` (main Factory).** Removes the `ProxySendPacket` ExecuteMsg variant and its handler. Adds a new reply ID for pool-factory delegations. Pool-handler stubs in `execute::pool` change their delegation from `SubMsg::new(WasmMsg::Execute{…})` (fire-and-forget) to `SubMsg::reply_on_success(WasmMsg::Execute{…}, POOL_FACTORY_DELEGATE_REPLY_ID)`. The reply handler decodes pool factory's returned data and runs `execute_send_packet`. `execute_send_packet` itself is unchanged.

**`pool_factory`.** Each `On*` handler that previously emitted a `FactoryExecuteMsg::ProxySendPacket` SubMsg stops doing so and instead returns a `Response` with `data: Some(to_json_binary(&PoolFactoryReply::SendPacket{…})?)`. The `outbound` builder module is unchanged — pool factory still owns packet construction. Pool factory no longer needs to construct `FactoryExecuteMsg::ProxySendPacket`; it does not import `FactoryExecuteMsg` for this purpose. The `PoolFactoryReply` type is defined in the shared `euclid` package alongside the existing pool-factory message types so both contracts depend on the same typed shape.

### Data shape

```rust
// packages/euclid/src/msgs/pool_factory/reply.rs (new)
#[cw_serde]
pub enum PoolFactoryReply {
    SendPacket {
        msg: Binary,                  // serialised RouterCrossChainExecuteMsg
        timeout: Option<u64>,
        ack_response: Option<Binary>,
        sender: Addr,                 // original user; used by execute_send_packet
    },
}
```

Single-variant today, deliberately an enum so future reply-driven actions (e.g., a `MintLpToken` variant that lets us retire `ProxyMintLpToken` next) are additive. Enum extension is non-breaking provided the reply handler matches exhaustively and treats unknown variants as an error.

### Flow

Canonical example — CP `AddLiquidity`, post-change:

1. User calls `factory::execute(AddLiquidity{…})` with attached funds.
2. Main Factory validates, generates `tx_id`, emits escrow-deposit SubMsg as `reply_on_success(ESCROW_DEPOSIT_REPLY_ID)`.
3. Escrow deposit succeeds; reply handler builds `pool_factory::ExecuteMsg::OnAddLiquidity{tx_id, sender, …}` and emits it as `SubMsg::reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`.
4. Pool factory verifies `info.sender == MAIN_FACTORY_ADDRESS`, writes `PENDING_ADD_LIQUIDITY[(sender, tx_id)]`, builds outbound binary via `outbound::add_liquidity`, returns `Response::new().add_attribute(…).set_data(to_json_binary(&PoolFactoryReply::SendPacket{msg, timeout, ack_response, sender})?)`. No outbound message is emitted from pool factory's response.
5. Main Factory's reply handler with `POOL_FACTORY_DELEGATE_REPLY_ID`:
   - Reads `msg.result.into_result()?.data`, errors if `None`.
   - Decodes as `PoolFactoryReply`.
   - For `SendPacket`: deserialises `msg` to `RouterCrossChainExecuteMsg`, asserts `is_pool_variant(&decoded)`, calls `execute_send_packet(deps, env, info, msg, timeout, ack_response, sender)` exactly as `execute_proxy_send_packet` does today.
6. (Later) Ack arrives at main Factory's IBC entry; the existing pool-variant forward to `pool_factory::OnPoolAck` is unchanged. The downstream `Proxy*` calls from pool factory (mint LP, release escrow, etc.) are unchanged.

For pool-creation, remove-liquidity, and CLP creation, the structure is identical: thin stub → escrow/funds path (where applicable) → `reply_on_success` to pool factory's `On*` handler → reply data carries the outbound packet → main Factory sends it.

### Authorisation model (delta from original plan)

- Main Factory's pool-related ExecuteMsg variants: public, unchanged.
- **Main Factory's `ProxySendPacket` entry: REMOVED.**
- Main Factory's other `Proxy*` entries (`ProxyMintLpToken`, `ProxyBurnLpToken`, `ProxyTransferLpToken`, `ProxyReleaseEscrow`, `ProxyMintPosition`, `ProxyUpdatePosition`, `ProxyBurnPosition`): unchanged — auth-gated to `info.sender == POOL_FACTORY_ADDRESS` as before.
- Pool factory's `On*` and `OnPoolAck` entries: unchanged — auth-gated to `info.sender == MAIN_FACTORY_ADDRESS` as before.
- Pool factory's `MigrateAcceptPoolState`: unchanged.
- The reply handler on main Factory has no sender check (none exists for replies in CosmWasm; the VM guarantees the reply corresponds to a submsg main Factory itself dispatched).

### Defence in depth in the reply handler

Even though the reply scoping is structural, the handler still validates the decoded `RouterCrossChainExecuteMsg` is one of `{RequestPoolCreation, RequestConcentratedPoolCreation, AddLiquidity, AddConcentratedLiquidity, RemoveLiquidity, RemoveConcentratedLiquidity, CollectConcentratedFees, CollectConcentratedProtocolFees}`. Reuses or extends the same `is_pool_variant` matcher already used for the inbound ack forward in `reusable_internal_ack_call`. A non-pool variant returns `ContractError::Unauthorized {}` (or a more specific error) and the tx reverts.

### Reply handler error semantics

- `msg.result.is_err()` → cannot happen because dispatch uses `reply_on_success`; if pool factory errors, the whole tx reverts before the reply fires. No code path needed.
- `msg.result.into_result()?.data == None` → error: pool factory must always set data when invoked as a delegated handler. (No legitimate path on the delegated handlers omits data.)
- Decode of `PoolFactoryReply` fails → error.
- `is_pool_variant` returns false → error.
- All errors abort the outer tx; escrow funds (if any) revert with it; LP tokens held in cw20 escrow revert with it.

### Retrofit sequence

The change lands in five PRs aligned with the already-landed slices:

1. **PR A — infrastructure.** Add `PoolFactoryReply` enum, `POOL_FACTORY_DELEGATE_REPLY_ID` constant, reply handler arm on main Factory, helper `is_pool_variant` (or extend the ack-side one). Keep `ProxySendPacket` alive in this PR — no behaviour change yet.
2. **PR B — retrofit Slice 1 (CP create).** Switch `on_request_pool_creation` to return `PoolFactoryReply::SendPacket`. Switch main Factory's CP-create stub dispatch to `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. Existing integration test `pool_factory_cp_create` is the regression gate.
3. **PR C — retrofit Slice 2 (CP add).** Same change shape for `on_add_liquidity`.
4. **PR D — retrofit Slice 3 (CP remove).** Same change shape for `on_remove_liquidity`.
5. **PR E — retrofit Slice 4 (CLP create) + remove `ProxySendPacket`.** Switch `on_request_concentrated_pool_creation`. With the last caller converted, delete `ProxySendPacket` ExecuteMsg variant, `execute_proxy_send_packet`, its handler dispatch, and its unit tests. Schemas regenerate.

Future Slices 5–7 (CLP add, fees, remove) are designed under the new pattern from the start — no second migration.

PR ordering is strict (A before all others; B–D order-independent among themselves; E last). Each PR ships green CI and updates `CHANGELOG.md`.

### Schema and API contract changes

- `factory::ExecuteMsg`: `ProxySendPacket` variant removed by PR E. **Schema-breaking** for any external caller of that variant — but the only intended caller was pool factory, and pool factory is part of the same release. No frontend/SDK impact.
- `factory::QueryMsg`: unchanged.
- `pool_factory::ExecuteMsg`: unchanged.
- `pool_factory::QueryMsg`: unchanged.
- New shared type `euclid::msgs::pool_factory::reply::PoolFactoryReply` exported from the `euclid` package.

### State, events, tx attributes

- No new persistent state on either contract.
- Existing event names and tx attributes (`tx_id`, `TxType`, `simple_event`, `tx_event`, `action`, `method`) for pool flows preserved. The `method=proxy_send_packet` attribute is no longer emitted (the handler that emitted it is deleted); the upstream `method=*_request_delegated` attributes already emitted by main Factory's pool stubs continue to mark the delegated path for indexers. Indexers that watched `method=proxy_send_packet` should switch to the upstream `method=*_request_delegated` markers — call out in the `CHANGELOG.md` entry.

### Migration / bootstrap

Unchanged from the original plan. The drain-and-cut migration (Slice 8) does not touch `ProxySendPacket` — it's already gone by then. `SetPoolFactory` admin entry on main Factory and `MAIN_FACTORY_ADDRESS` on pool factory continue to serve the same role: pool factory still needs to know main Factory's address for its `On*` auth check, and main Factory still needs to know pool factory's address for `Proxy*` (mint/burn/release) auth checks.

## Testing Decisions

**What makes a good test here.** Tests assert external behaviour at contract boundaries: emitted submessages and their payloads, returned errors, post-call state queryable through public queries, and — new — `Response::data` payloads on the pool factory side. Tests do not assert internal storage layout or specific helper call sequences. The reply handler's auth boundary is structural; the tests verify the *content* gates (data present, decode succeeds, variant is a pool variant).

**Existing prior art.**

- The currently passing `pool_factory_cp_create`, `pool_factory_cp_add_liquidity`, `pool_factory_cp_remove_liquidity`, and `pool_factory_clp_create` integration tests (all three chain modes via rstest) are the regression suite for the retrofit. They are not modified by this PRD; if any of them break, the retrofit is wrong.
- Existing `execute_proxy_send_packet` unit tests (`test_proxy_send_packet_unauthorised_caller_rejected`, `test_proxy_send_packet_with_no_pool_factory_set_unauthorised`, `test_proxy_send_packet_authorised_caller_emits_submsg` in `factory/src/execute/proxy.rs`) are removed in PR E along with the function. Their coverage is replaced by the new reply-handler tests below.
- `pool_factory::execute::cp` and `pool_factory::execute::clp` unit tests already verify each `On*` handler's happy path and unauthorised-caller path; they are extended to assert `Response::data` content.

**Modules to test.**

- **`pool_factory::execute::cp` and `pool_factory::execute::clp` handler tests (extended).** For each handler retrofitted (`on_request_pool_creation`, `on_add_liquidity`, `on_remove_liquidity`, `on_request_concentrated_pool_creation`), add an assertion that the returned `Response.data` decodes as `PoolFactoryReply::SendPacket` with the expected `msg`, `timeout`, `ack_response`, and `sender`. Decode the inner `msg` and assert the `RouterCrossChainExecuteMsg` variant matches.
- **`factory` reply handler unit tests (new).** Synthetic `Reply` messages with `POOL_FACTORY_DELEGATE_REPLY_ID` and:
  - Valid `PoolFactoryReply::SendPacket` payload with a pool variant → asserts an IBC SubMsg (or native callback) is emitted with the right binary, sequence, timeout, ack.
  - `data == None` → returns an error; no submsg emitted.
  - Garbage data that does not decode as `PoolFactoryReply` → returns an error.
  - `PoolFactoryReply::SendPacket` whose inner binary decodes to a *non-pool* `RouterCrossChainExecuteMsg` variant (e.g., `Swap`) → returns an error; no submsg emitted. This is the defence-in-depth check.
- **`factory::ExecuteMsg` surface check.** Compile-time / schema-snapshot test ensures `ProxySendPacket` is gone post-PR E.
- **Integration suite.** All four existing `pool_factory_*` end-to-end tests continue to pass unchanged across Native, IBC, and EVM modes. No new integration test is required — the retrofit's correctness is observable in the existing assertions (LP balance increase, escrow update, position NFT mint, event emission). Add a single new negative integration test: simulate a pool_factory bug by deploying a malicious test-only stub that returns a `Swap` packet in its reply data; assert main Factory's reply handler rejects the tx and no IBC packet is emitted. (This test lives in the integration suite because it requires exercising the cross-contract reply chain end-to-end.)

## Out of Scope

- Removing the other `Proxy*` entries on main Factory (`ProxyMintLpToken`, `ProxyBurnLpToken`, `ProxyTransferLpToken`, `ProxyReleaseEscrow`, `ProxyMintPosition`, `ProxyUpdatePosition`, `ProxyBurnPosition`). Their attack surface is narrower (scoped to the named resource); converting them to reply-data costs more in complexity than it gains and can be done as a follow-up if SC-6 audit recommends it.
- Changes to `OnPoolAck` dispatch. The inbound ack path remains a `WasmMsg::Execute` SubMsg from main Factory to pool factory; pool factory still calls back via the remaining `Proxy*` entries.
- Changes to `RouterCrossChainExecuteMsg`, `FactoryCrossChainExecuteMsg`, or any router-side code.
- Changes to the per-chain drain-and-cut migration (Slice 8) or to bootstrap flow.
- Changes to LP token / position token contracts, escrow contracts, or the virtual balance / voucher subsystem.
- Re-ordering or merging of the existing SC-4 slices (1–9). This PRD adds five retrofit PRs **inside the existing pools-functions-refactor branch**; the slice plan itself is preserved.

## Further Notes

- The original Slice 1 acceptance criterion that called out `ProxySendPacket` and its unit tests becomes "verified absent" by PR E. Update `POOL_FACTORY_REFACTOR_ISSUES.md`'s status table once PR E lands to note the entry has been removed.
- The defence-in-depth `is_pool_variant` matcher is the same one used by `reusable_internal_ack_call` on the inbound path. Centralising it (single function in `packages/euclid_ibc/src/router_ibc.rs` or a sibling helper module) keeps the two sites — outbound reply-handler validation, inbound ack dispatch — in lockstep when new pool variants are added later. If a new pool variant is added without updating the matcher, both ends fail closed (the outbound is rejected; the inbound falls through to the non-pool arm and errors). This is the desired failure mode.
- If a future PRD applies the same reply-data pattern to the mint/burn/release proxies, the `PoolFactoryReply` enum is the natural extension point: add `MintLpToken { … }`, `BurnLpToken { … }`, `ReleaseEscrow { … }` variants; pool factory's `OnPoolAck` returns one of those as `Response::data`; main Factory's reply handler dispatches by variant. The mechanics validated by this PRD generalise cleanly.
- `CHANGELOG.md` entry under the current Sirius "in progress" section:
  - `[factory]` security: `ProxySendPacket` ExecuteMsg variant removed; pool factory now communicates outbound IBC packets via `Response::data` consumed by main Factory's reply handler, eliminating a public arbitrary-IBC-packet entry point.
  - `[factory]` improvement: reply handler validates pool variant before invoking `execute_send_packet` (defence in depth).
  - `[pool_factory]` changed: `On*` handlers no longer emit `FactoryExecuteMsg::ProxySendPacket` submessages; the outbound packet is returned via `Response::data` typed as `PoolFactoryReply::SendPacket`.
- `MIGRATION.md` does not need an update — the on-chain state shape is unchanged.
