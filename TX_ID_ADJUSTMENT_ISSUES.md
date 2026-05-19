# tx_id Adjustment — Issue Breakdown

Vertical-slice breakdown of the CosmWasm portion of SC-8 (Block Reorg Fix — remove block height from txid format). Solana and Tron are out of scope for this branch.

## Parent

Linear SC-8 — Block Reorg Fix — remove block height from txid format.

## Background

The shared `generate_tx` helper in the `euclid` package produces the `tx_id` used as the key for `PENDING_SWAPS`, `PENDING_REMOVE_LIQUIDITY`, and `PENDING_RELEASE_VOUCHER` on both the Router and every Factory. The current format embeds `block.height` and `transaction.index`, both of which can shift across reorg replays. The failure mode is the ack-direction mismatch: source reorgs after the destination has already ack'd, replay produces a different `tx_id`, and the inbound ack's `PENDING_*` lookup misses by key, leaving the operation stuck.

The fix removes `block.height` and `transaction.index` from the format. After the change, every input to `generate_tx` is either external (`sender`), constant per chain (`chain_id`), or backed by atomically-rolled-back contract storage (`nonce`), so the same logical transaction reproduces the same `tx_id` across any replay.

### Known limitation (deferred)

`TX_NONCE` is a single global counter per contract, not per-sender. If a reorg replays a block containing multiple senders' transactions in a different order, each sender's nonce shifts and the determinism property breaks for the cross-sender reordering case. Mitigation is per-sender nonce (`Map<String, u128>` keyed by `sender.to_sender_string()`), which is reorg-deterministic in practice because Cosmos SDK enforces strict per-account sequence ordering. This is documented in code and CHANGELOG but **not** implemented in this branch; it is a deliberate follow-up.

---

## Slice 1 — Reorg-safe tx_id format with determinism contract

- **Type:** AFK
- **Blocked by:** None
- **User stories covered:** 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14

### What to build

Change `generate_tx` in the `euclid` package to produce `{sender}:{chain_id}:{nonce}` (drop `height` and `index`). Both Router and Factory pick up the change automatically via the shared package; no caller-side edits.

Add unit tests colocated with the helper that assert its determinism contract directly. Document the global-nonce limitation inline at the helper site and in `CHANGELOG.md` under `[euclid]`.

### Acceptance criteria

- [ ] `generate_tx` emits `{sender}:{chain_id}:{nonce}` — no `block.height` or `transaction.index` in the output.
- [ ] `generate_tx` signature, return type, and `TX_NONCE` storage layout unchanged.
- [ ] Unit test: same `(sender, chain_id, TX_NONCE)` across varying `block.height` and `transaction.index` produces the same id.
- [ ] Unit test: two back-to-back calls in the same `Env` produce different ids (nonce-disambiguation regression).
- [ ] Unit test: different `sender` values produce different ids.
- [ ] Unit test: different `block.chain_id` values produce different ids.
- [ ] Inline comment on `generate_tx` describes the global-nonce limitation, the Alice-then-Bob → Bob-then-Alice replay reproduction, and points to the per-sender-nonce follow-up.
- [ ] `CHANGELOG.md` entry under `[euclid]` in the current in-progress release describes both the format change and the deferred limitation.
- [ ] `cargo test -p euclid` passes.
- [ ] `cargo clippy --all-targets --all-features` clean for the touched package.

### Blocked by

- None — can start immediately.

---

## Slice 2 — Synthetic stuck-packet integration regression test

- **Type:** AFK
- **Blocked by:** Slice 1
- **User stories covered:** 5, 10 (regression coverage)

### What to build

Add synthetic stuck-packet tests that simulate the ack-direction failure mode by exercising the contract's ack/reply handlers with a `tx_id` that does not match any `PENDING_*` entry (representing the state a reorg-replayed source would produce). Assert the handler fails predictably with a defined error rather than silently mutating state.

The synthesis is exercised at the **router unit-test level** rather than in `tests-integration`. Rationale: the failure mode lives at the contract's storage layer (`.load()` on a missing key in `PENDING_SWAPS` / `PENDING_RELEASE_VOUCHER`). Router unit tests already have the `MockDeps`/seeded-`PENDING_*`/`ok_reply` infrastructure to drive that path directly, and `cw-orch-interchain` adds no fidelity over `MockDeps` here — neither environment can actually reorg. The unit-test version exercises the exact line that would fail under Q1(b), is faster, and avoids cw-orch ceremony that would obscure the test's intent.

### Acceptance criteria

- [x] `PENDING_SWAPS` missing-pending scenario: synthetic reply for an unknown `tx_id` is rejected with a contract error; the originally-seeded entry is untouched.
- [x] `PENDING_RELEASE_VOUCHER` missing-pending scenario: synthetic ack for an unknown `tx_id` is rejected with a contract error; the originally-seeded entry is untouched.
- [x] Tests carry comments tying them to the Q1(b) ack-direction failure mode.
- [x] `cargo test -p router` passes without regressions.

### Blocked by

- Slice 1 (the integration test asserts behavior under the new format).

---

## Out of scope on this branch

- Solana and Tron implementations (handled separately under SC-8 parent).
- Off-chain consumer (relayer, indexer, dashboards) updates to handle the 3-segment format.
- Per-sender nonce implementation (deferred follow-up; documented but not built).
- Migration of in-flight `PENDING_*` entries (none needed — old and new formats coexist via segment-count difference).
- Replacing `wrapping_add` on `TX_NONCE` with `checked_add`.
