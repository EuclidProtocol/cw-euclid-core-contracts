# Issues — Per-wallet Euclid fee override (SC-23, Implementation 1)

> Local issue breakdown (not published to Linear). Tracer-bullet vertical slices.
> Parent: [SC-23](https://linear.app/euclid-protocol/issue/SC-23/adding-custom-logic-to-exempt-mm-wallet-address-from-euclid-fees)
> PRD: `PRD-SC-23-euclid-fee-override.md`
> Suggested triage label when published: `feature` (no `ready-for-agent` label exists in the Smart Contracts team).

Dependency order: 1 → 2 → 3 → 4 → 5 → 6, with 7 (HITL) after all build slices.

---

## Issue 1 — Euclid-fee override: whitelist management, read query & resolver

**Type:** AFK

### Parent
SC-23

### What to build
The Router-side foundation for per-wallet Euclid-fee overrides. Add storage for a per-wallet override keyed by the components of the swapping `CrossChainUser` (chain + address), an admin message to manage it, a read query to inspect it, and a single shared resolver used by every consumer.

- A fee-admin-gated management message `SetEuclidFeeOverride { user, euclid_fee_bps: Option<u64> }`: `Some(bps)` upserts the override (validated against the max-fee bound); `None` removes the entry. Emits a dedicated indexing event carrying the affected chain, address, and the new value (or a removal marker).
- A read query `GetEuclidFeeOverride { user } -> Option<u64>` for backend/admin auditing.
- A deep resolver helper that, given storage and a `CrossChainUser`, returns the optional override. This is the single resolution point reused by the execute and simulate paths so they cannot drift.

No swap behavior changes yet — this slice only manages and exposes the whitelist. The map starts empty, so existing behavior is unchanged and no state migration is required.

### Acceptance criteria
- [x] Fee admin can set an override for a wallet; reading it back returns the stored value.
- [x] Setting an override with `None` removes the entry; reading it back returns no override.
- [x] Non-fee-admin senders (general admin, migration admin, arbitrary addresses) are rejected.
- [x] Override values above the max-fee bound are rejected; `0` is accepted (full exemption).
- [x] Overrides are keyed per (chain, address): the same address on two different chains resolves independently.
- [x] An indexing event is emitted on set and on remove, with correct attributes.
- [x] The resolver returns the stored value when present and none when absent (unit-tested over a mock store).
- [x] No change to swap or simulation behavior; empty map means current behavior.

### Status
**Done.** Storage map, message variants, fee-admin-gated handler, query, and resolver helper landed on branch `joe/sc-23-euclid-fee-override`. 15 unit tests passing (5 resolver, 9 handler, 1 query roundtrip).

### Blocked by
None — can start immediately.

---

## Issue 2 — Apply Euclid-fee override on a single-hop CP swap

**Type:** AFK

### Parent
SC-23

### What to build
Make a whitelisted wallet actually pay the reduced (or zero) Euclid fee on a single-hop constant-product swap, end-to-end. This slice introduces the swap-message plumbing and the injection point that later slices reuse.

- Add a single sender-level optional override field to the router→VLP swap message.
- At the single Router swap chokepoint (where native and IBC swaps both converge and the initial swap message is built), resolve the override for the swap's sender via the Issue 1 resolver and stamp it onto the outgoing message.
- Apply the override in the CP pre-swap fee calculation: when present it replaces the Euclid-fee rate; when absent, behavior is identical to today. The LP fee is always charged at the pool rate.

### Acceptance criteria
- [x] A whitelisted wallet's single-hop CP swap charges the override Euclid fee (`0` → no Euclid fee; reduced bps → reduced fee), increasing the swapped amount accordingly.
- [x] A non-whitelisted wallet's CP swap is unchanged (full Euclid fee).
- [x] The LP fee is identical with and without an override.
- [x] The override applies identically for swaps originating from native chains and IBC chains.
- [x] A wallet whose entry was removed reverts to the pool's configured Euclid fee.
- [x] Override-aware CP fee math is covered by table-driven unit tests (exemption, reduced, none/default, max-fee boundary, LP-fee-unchanged).

### Status
**Done.** Added `euclid_fee_override: Option<u64>` (serde-defaulted) to `VlpSwapMsg`; the Router resolves it via the Issue 1 resolver at the single `ibc_execute_swap` chokepoint (shared by native + IBC) and stamps it onto the outgoing message. `pre_swap` applies it (replaces the Euclid-fee rate; LP fee stays at pool rate); `execute_swap` forwards it to the next hop. Removed entries resolve to `None`, reverting to the pool fee. 7 new table-driven CP fee-math unit tests in `euclid-pool` (full exemption, reduced, none/default, override==pool, max-fee boundary, exemption-increases-out, reduced-between).

### Blocked by
- Issue 1

---

## Issue 3 — Apply Euclid-fee override on a single-hop stable swap

**Type:** AFK

### Parent
SC-23

### What to build
Extend the override application to the stable-swap curve, reusing the message field and injection built in Issue 2. Only the stable pre-swap fee calculation needs the override parameter wired in.

### Acceptance criteria
- [x] A whitelisted wallet's single-hop stable swap charges the override Euclid fee; non-whitelisted is unchanged.
- [x] The LP fee on stable swaps is identical with and without an override.
- [x] A removed entry reverts to the pool's configured Euclid fee.
- [x] Override-aware stable fee math is covered by table-driven unit tests (exemption, reduced, none/default, max-fee boundary, LP-fee-unchanged).

### Status
**Done.** The stable VLP already passes `swap_msg.euclid_fee_override` into the shared `execute_swap`/`pre_swap` (wired in Issue 2), so the override drives the stable curve identically — fees are computed before the curve, so the math is curve-independent. Added 6 stable-curve unit tests in `euclid-pool` mirroring the CP table (full exemption, reduced, none/default, override==pool, max-fee boundary, plus exemption-increases-out). No production code change beyond Issue 2.

### Blocked by
- Issue 2

---

## Issue 4 — Forward Euclid-fee override across multi-hop CP/stable routes

**Type:** AFK

### Parent
SC-23

### What to build
Make the override survive every hop of a multi-hop route. Each VLP rebuilds the swap message for the next hop forwarding the sender; forward the override field the same way so it applies uniformly across all CP/stable hops.

### Acceptance criteria
- [ ] A multi-hop CP/stable route by a whitelisted wallet charges the override Euclid fee on every hop, not just the first.
- [ ] A non-whitelisted wallet's multi-hop route is unchanged on every hop.
- [ ] Integration test covers a route with at least two CP/stable hops end-to-end.

### Blocked by
- Issue 2
- Issue 3

---

## Issue 5 — Concentrated pools: forward override, do not apply

**Type:** AFK

### Parent
SC-23

### What to build
Define concentrated-pool behavior for the override. Because the concentrated Euclid value is the protocol's *cut* of the LP fee (not an additive trader fee), applying it would not improve the wallet's quote — it would only shift the protocol's cut to LPs. So the concentrated VLP must **forward** the override field to downstream hops but **not apply** it to its own protocol cut. Leave a documented TODO marking where meaningful concentrated exemption (reducing the structural fee tier) would later be added.

### Acceptance criteria
- [ ] A route of CP → concentrated → CP by a whitelisted wallet is exempt on the CP legs and charged normally on the concentrated leg.
- [ ] The concentrated leg's protocol cut and LP allocation are unchanged whether or not the wallet has an override.
- [ ] The override field is forwarded by the concentrated VLP to the next hop.
- [ ] A documented TODO marks the deferred concentrated fee-tier exemption.

### Blocked by
- Issue 4

---

## Issue 6 — Sender-aware swap simulation (quote == execution)

**Type:** AFK

### Parent
SC-23

### What to build
Make quotes reflect the override deterministically. Add an optional sender to the swap simulation request; the Router resolves the override (via the Issue 1 resolver) and threads it through the simulation path into the same pre-swap fee logic used by execution, so the simulated Euclid fee equals the executed fee bit-for-bit — across single-hop, multi-hop, and the concentrated forwarding rule. The sender field is optional for backward compatibility (absent = current behavior).

### Acceptance criteria
- [ ] Simulating a swap with a whitelisted sender returns a quote whose Euclid fee equals what execution charges, for a single-hop CP/stable swap.
- [ ] The same parity holds for a multi-hop CP/stable route.
- [ ] Simulating a route through a concentrated hop matches execution (concentrated leg charged normally).
- [ ] Simulating without a sender returns the current (full-fee) behavior.
- [ ] Integration test asserts simulated fee == executed fee for whitelisted and non-whitelisted senders.

### Blocked by
- Issue 5

---

## Issue 7 — Decide and implement override rollout / version-gating

**Type:** HITL

### Parent
SC-23

### What to build
Settle the deferred rollout decision (flagged as an open item in the PRD). The new swap-message and simulate fields are optional, so old/new contract combinations degrade gracefully — worst case a whitelisted wallet is charged the normal fee, never an error or fund issue. The exemption only takes effect once both the Router (injects) and the target VLPs (apply) are upgraded.

Decide between:
- **Graceful fallback** — rely on optional fields + serde defaults; rollout order is upgrade VLPs, then Router; accept that exempt wallets pay normal fees during the rollout window.
- **Hard version gate** — reject swaps until all contracts are upgraded.

Then implement the chosen approach (or document graceful fallback as the accepted behavior if no gate is added).

### Acceptance criteria
- [ ] Decision recorded (graceful fallback vs. hard version gate) with rationale.
- [ ] If a gate is chosen: swaps are rejected/handled per the decision until all contracts are upgraded, with tests.
- [ ] If graceful fallback is chosen: the fallback behavior is documented and a test confirms mixed-version combinations never error and never touch LP fees.

### Blocked by
- Issue 2
- Issue 3
- Issue 4
- Issue 5
- Issue 6
