# Pool Factory Refactor — Reply-Data Send-Packet Auth (Issues)

Tracer-bullet slices for the SC-4 amendment removing `ProxySendPacket` in favour of reply-data delegation.

Parent: [SC-4 — Pools Function Refactor](https://linear.app/euclid-protocol/issue/SC-4/pools-function-refactor)
Source PRD: [POOL_FACTORY_REFACTOR_REPLY_DATA.md](./POOL_FACTORY_REFACTOR_REPLY_DATA.md)
Original plan (unchanged): [POOL_FACTORY_REFACTOR.md](./POOL_FACTORY_REFACTOR.md) · [POOL_FACTORY_REFACTOR_ISSUES.md](./POOL_FACTORY_REFACTOR_ISSUES.md)
Branch: `pools-functions-refactor`

This amendment retrofits the four already-landed SC-4 slices (1–4) so that pool factory communicates outbound IBC packets via `Response::data` instead of calling `main_factory::ProxySendPacket`. The original Slices 5–9 are not yet implemented; they will be designed under the new pattern from the start and are out of scope for this amendment.

## Dependency graph

```
PR A (infra: PoolFactoryReply, reply handler, is_pool_variant)
 ├── PR B (retrofit Slice 1 — CP create)
 ├── PR C (retrofit Slice 2 — CP add)
 ├── PR D (retrofit Slice 3 — CP remove)
 └── PR E (retrofit Slice 4 — CLP create + delete ProxySendPacket + negative test)
        ↑ also blocked by B, C, D
```

PRs B/C/D are order-independent among themselves. PR E lands last because deleting `ProxySendPacket` requires every caller switched first.

## Status

| PR | Status | Notes |
|---:|:-------|:------|
| A | ⬜ Ready to start | Pure additive infra; no behaviour change. |
| B | ⬜ Blocked by A | Retrofits the CP-create path landed in commit `69d2740c`. |
| C | ⬜ Blocked by A | Retrofits the CP add-liquidity path landed in commit `…` (Slice 2). |
| D | ⬜ Blocked by A | Retrofits the CP remove-liquidity path landed in commits `7dd4c0d1`, `d55dbdd7`. |
| E | ⬜ Blocked by A, B, C, D | Retrofits CLP-create (Slice 4) and deletes `ProxySendPacket` end-to-end. |

---

## PR A — Reply-data infrastructure

**Type:** AFK
**Blocked by:** None — can start immediately.

### What to build

Stand up the typed reply-data path between pool factory and main Factory without changing any existing behaviour. Three pieces:

1. **Shared reply type.** A new `PoolFactoryReply` enum in the `euclid` package, single variant `SendPacket { msg: Binary, timeout: Option<u64>, ack_response: Option<Binary>, sender: Addr }`. Deliberately an enum so future variants (e.g., `MintLpToken`, `BurnLpToken`, `ReleaseEscrow`) can be added without breaking the wire shape.

2. **Reply handler on main Factory.** New constant `POOL_FACTORY_DELEGATE_REPLY_ID` and a matching arm in main Factory's `reply` entry that:
   - Reads `msg.result.into_result()?.data`, errors if `None`.
   - Decodes the data as `PoolFactoryReply`.
   - For `SendPacket`: deserialises the inner binary into `RouterCrossChainExecuteMsg`, asserts it is a pool variant via `is_pool_variant`, and runs the same logic `execute_proxy_send_packet` runs today (which is the existing `execute_send_packet` flow).
   - Errors with `ContractError::Unauthorized {}` (or a more specific error) on any of: missing data, undecodable data, non-pool variant.

3. **Centralised `is_pool_variant` matcher.** Extract into `packages/euclid_ibc/` (or a sibling helper module under the same crate) the matcher that returns `true` for the eight pool variants: `RequestPoolCreation`, `RequestConcentratedPoolCreation`, `AddLiquidity`, `AddConcentratedLiquidity`, `RemoveLiquidity`, `RemoveConcentratedLiquidity`, `CollectConcentratedFees`, `CollectConcentratedProtocolFees`. Update the existing inbound ack-side caller in `reusable_internal_ack_call` to use the centralised helper so both ends share a single source of truth.

This PR is purely additive. `ProxySendPacket` and its handler remain in place; nothing routes through the new reply ID yet. The integration suite (all four `pool_factory_*` tests) must remain green to prove the additive work has no behavioural effect.

### Acceptance criteria

- [ ] `PoolFactoryReply::SendPacket` exists in `packages/euclid` (alongside other `pool_factory` message types) and is exported.
- [ ] `POOL_FACTORY_DELEGATE_REPLY_ID` constant lives with the other factory reply IDs.
- [ ] Main Factory's `reply` entry has a new arm for `POOL_FACTORY_DELEGATE_REPLY_ID` implementing the decode/validate/dispatch logic above.
- [ ] `is_pool_variant` is defined once (in `packages/euclid_ibc/` or a sibling helper) and used by both the inbound ack forward and the new reply handler.
- [ ] Unit tests on main Factory's reply handler:
  - happy path with a valid `PoolFactoryReply::SendPacket` carrying a pool variant emits the expected IBC SubMsg (or native callback) with the right binary, timeout, ack response, and sender;
  - `data == None` returns an error and emits no submsg;
  - garbage bytes that do not decode as `PoolFactoryReply` return an error;
  - `PoolFactoryReply::SendPacket` whose inner binary is a non-pool variant (e.g., `Swap`) returns an error and emits no submsg.
- [ ] All four existing `pool_factory_*` integration tests pass unchanged in Native, IBC, and EVM modes.
- [ ] `ProxySendPacket` and `execute_proxy_send_packet` remain in place — no behaviour change in this PR.
- [ ] `cargo fmt --all -- --check`, `cargo clippy -- -W clippy::pedantic`, and `cargo unit-test --locked` all pass.
- [ ] `CHANGELOG.md` updated with `[factory]` and `[euclid]` entries noting the additive infra under the current Sirius in-progress section.

---

## PR B — Retrofit CP pool creation (Slice 1)

**Type:** AFK
**Blocked by:** PR A

### What to build

Switch the CP/Stable pool creation path landed in Slice 1 from the `ProxySendPacket` round-trip to reply-data delegation.

- **Pool factory.** `on_request_pool_creation` stops constructing and emitting a `FactoryExecuteMsg::ProxySendPacket` submsg. On the success path it builds the outbound packet via `outbound::request_pool_creation` as today, then returns a `Response` with `data: Some(to_json_binary(&PoolFactoryReply::SendPacket { msg: packet, timeout, ack_response, sender })?)` and no outbound message. Attributes (including `method=on_request_pool_creation` and `tx_id`) are preserved.
- **Main Factory.** The CP-create stub in `execute::pool` changes the SubMsg from `SubMsg::new(WasmMsg::Execute{…})` (fire-and-forget) to `SubMsg::reply_on_success(WasmMsg::Execute{…}, POOL_FACTORY_DELEGATE_REPLY_ID)`. The `method=request_pool_creation_delegated` attribute is preserved.

The `ProxySendPacket` variant remains alive in this PR — only this caller stops using it.

### Acceptance criteria

- [ ] `pool_factory::on_request_pool_creation` returns `Response::data` typed as `PoolFactoryReply::SendPacket` instead of emitting a `FactoryExecuteMsg::ProxySendPacket` submsg. No reference to `FactoryExecuteMsg::ProxySendPacket` remains in this handler.
- [ ] Main Factory's CP-create delegate SubMsg uses `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`.
- [ ] `pool_factory_cp_create` integration test passes unchanged in Native, IBC, and EVM modes.
- [ ] Unit test on `on_request_pool_creation` asserts the returned `Response.data` decodes as `PoolFactoryReply::SendPacket`, the inner binary decodes as `RouterCrossChainExecuteMsg::RequestPoolCreation`, and the `sender`/`timeout`/`ack_response` fields match the inputs.
- [ ] Existing unauthorised-caller and happy-path unit tests on `on_request_pool_creation` continue to pass.
- [ ] Event/tx-attribute parity for CP pool creation flow.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## PR C — Retrofit CP add_liquidity (Slice 2)

**Type:** AFK
**Blocked by:** PR A

### What to build

Identical change shape to PR B, applied to `pool_factory::on_add_liquidity`.

- **Pool factory.** Drop the `FactoryExecuteMsg::ProxySendPacket` submsg construction; return `Response::data` typed as `PoolFactoryReply::SendPacket` carrying the `outbound::add_liquidity` binary plus the incoming `timeout`, `ack_response`, and `sender`.
- **Main Factory.** `add_liquidity_request_delegated` (the escrow-deposit-reply-driven path) changes its delegate SubMsg from fire-and-forget to `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. Funds are still deposited to escrow before the IBC packet is sent — that ordering is preserved by the existing reply chain.

The `method=add_liquidity_request_delegated` attribute is preserved.

### Acceptance criteria

- [ ] `pool_factory::on_add_liquidity` returns `Response::data` typed as `PoolFactoryReply::SendPacket`. No `FactoryExecuteMsg::ProxySendPacket` references remain in this handler.
- [ ] Main Factory's add-liquidity delegate SubMsg uses `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`.
- [ ] Escrow deposit still completes before the IBC packet is emitted — verified by the existing `pool_factory_cp_add_liquidity` integration test (which asserts post-call balances).
- [ ] `pool_factory_cp_add_liquidity` integration test (including the IBC + EVM slippage-refund cases) passes unchanged.
- [ ] Unit test asserts `on_add_liquidity` returns the expected `Response.data` shape; existing unauthorised-caller and slippage tests continue to pass.
- [ ] Event/tx-attribute parity.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## PR D — Retrofit CP remove_liquidity (Slice 3)

**Type:** AFK
**Blocked by:** PR A

### What to build

Identical change shape applied to `pool_factory::on_remove_liquidity`.

- **Pool factory.** Drop the `FactoryExecuteMsg::ProxySendPacket` submsg; return `Response::data` typed as `PoolFactoryReply::SendPacket` carrying the `outbound::remove_liquidity` binary, `timeout`, `ack_response`, and `sender`.
- **Main Factory.** `remove_liquidity_request_delegated` switches its delegate SubMsg to `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. The cw20-receive entry point that triggers this flow is unchanged. The `VLP_TO_LP_SHARES` mirror update (Slice 3 carry-over) still happens on a successful ack, on pool factory, unchanged by this PR.

The `method=remove_liquidity_request_delegated` attribute is preserved.

### Acceptance criteria

- [ ] `pool_factory::on_remove_liquidity` returns `Response::data` typed as `PoolFactoryReply::SendPacket`. No `FactoryExecuteMsg::ProxySendPacket` references remain in this handler.
- [ ] Main Factory's remove-liquidity delegate SubMsg uses `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`.
- [ ] `pool_factory_cp_remove_liquidity` integration test passes unchanged in Native, IBC, and EVM modes.
- [ ] Unit test asserts `on_remove_liquidity` returns the expected `Response.data` shape; existing unauthorised-caller and ack-failure-path tests continue to pass.
- [ ] Event/tx-attribute parity.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked` pass.
- [ ] `CHANGELOG.md` updated.

---

## PR E — Retrofit CLP create + delete ProxySendPacket + negative test

**Type:** AFK
**Blocked by:** PR A, PR B, PR C, PR D

### What to build

The final PR: switch the last caller, delete the dead variant, ship the defence-in-depth integration test.

**Retrofit `on_request_concentrated_pool_creation`.** Identical change shape to the prior three PRs — pool factory returns `Response::data` typed as `PoolFactoryReply::SendPacket`; main Factory's delegated CLP-create stub uses `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`. The `method=request_concentrated_pool_creation_delegated` attribute is preserved.

**Delete `ProxySendPacket` end-to-end.** With every caller converted, remove:

- The `ProxySendPacket` variant from `factory::ExecuteMsg`.
- `execute_proxy_send_packet` and `handle_proxy_send_packet` in `factory/src/execute/proxy.rs`.
- The three unit tests covering the deleted handler (`test_proxy_send_packet_unauthorised_caller_rejected`, `test_proxy_send_packet_with_no_pool_factory_set_unauthorised`, `test_proxy_send_packet_authorised_caller_emits_submsg`).
- Any references in pool factory or shared messages that still mention `ProxySendPacket`.

Regenerate JSON schemas (`cargo schema` on the factory contract) so the breaking ExecuteMsg surface change is reflected in checked-in artefacts.

**Negative integration test.** Add a new integration test that exercises the defence-in-depth pool-variant check on main Factory's reply handler:

- Deploy a malicious test-only stub contract that mimics the pool factory's `On*` ExecuteMsg surface for the smallest applicable handler (e.g., `OnRequestPoolCreation`), but returns a `Response::data` whose inner binary is a non-pool `RouterCrossChainExecuteMsg::Swap` variant.
- Wire the stub as `POOL_FACTORY_ADDRESS` on a freshly deployed main Factory (using the existing `SetPoolFactory` bootstrap entry).
- Drive a user-facing pool-creation tx through main Factory. Assert the tx errors at the reply-handler stage and that no IBC packet (or native router callback) is emitted.

Test lives in `tests-integration/` so it exercises the real cross-contract reply chain. Single rstest case is sufficient — the variant rejection is chain-mode-independent — but parameterising across the three modes is also acceptable.

### Acceptance criteria

- [ ] `pool_factory::on_request_concentrated_pool_creation` returns `Response::data` typed as `PoolFactoryReply::SendPacket`. No `FactoryExecuteMsg::ProxySendPacket` references remain in this handler.
- [ ] Main Factory's CLP-create delegate SubMsg uses `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`.
- [ ] `pool_factory_clp_create` integration test passes unchanged in Native, IBC, and EVM modes.
- [ ] `factory::ExecuteMsg::ProxySendPacket` variant removed; `execute_proxy_send_packet` and `handle_proxy_send_packet` deleted; the three associated unit tests deleted.
- [ ] `cargo schema` regenerated; schema artefacts checked in; the diff shows `ProxySendPacket` removed with no additions.
- [ ] No remaining references to `ProxySendPacket` / `execute_proxy_send_packet` / `proxy_send_packet` anywhere in `contracts/`, `packages/`, or `tests-integration/` (rg sanity check).
- [ ] New integration test deploys a malicious stub that returns a non-pool packet in reply data; the test asserts main Factory's reply handler rejects the tx and emits no IBC packet.
- [ ] All four existing `pool_factory_*` integration tests still pass unchanged.
- [ ] `cargo fmt`, `cargo clippy -- -W clippy::pedantic`, `cargo unit-test --locked`, `cargo wasm --locked` all pass.
- [ ] `CHANGELOG.md` updated with:
  - `[factory]` security entry: `ProxySendPacket` removed; pool factory now communicates outbound packets via `Response::data`.
  - `[factory]` improvement entry: reply handler validates pool variant before invoking `execute_send_packet`.
  - `[pool_factory]` changed entry: `On*` handlers no longer emit `FactoryExecuteMsg::ProxySendPacket` submessages.
- [ ] Note added to `POOL_FACTORY_REFACTOR_REPLY_DATA_ISSUES.md` (this file) status table reflecting completion.

---

## Carry-over for not-yet-implemented slices

The original [POOL_FACTORY_REFACTOR_ISSUES.md](./POOL_FACTORY_REFACTOR_ISSUES.md) Slices 5 (CLP add), 6 (CLP fees), 7 (CLP remove), 8 (drain-and-cut migration), and 9 (query surface + schemas + docs) have not been implemented yet. After this amendment lands, those slices will be implemented under the reply-data pattern from the start:

- Their `On*` handlers will return `Response::data` typed as `PoolFactoryReply::SendPacket` (or a future enum variant) instead of emitting a `ProxySendPacket` submsg.
- Main Factory's user-facing stubs for those flows will dispatch their delegate SubMsg with `reply_on_success(POOL_FACTORY_DELEGATE_REPLY_ID)`.
- No second migration is required for those slices.

The original file is unchanged; this carry-over is the canonical reference for how the not-yet-landed slices interact with the new pattern.
