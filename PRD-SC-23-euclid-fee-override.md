# PRD — Per-wallet Euclid fee override (MM exemption), Router-level injection

> Source issue: [SC-23](https://linear.app/euclid-protocol/issue/SC-23/adding-custom-logic-to-exempt-mm-wallet-address-from-euclid-fees) — Implementation approach **1 (Router / centralized)**.
> Status: ready for implementation. Local PRD (not published to Linear).

## Problem Statement

Euclid charges a protocol fee (the **Euclid fee**) on every swap, on top of the LP fee. Market-maker (MM) wallets running arbitrage and RFQ strategies need better quotes than external market makers to win flow and keep Euclid's liquidity competitive. Today there is no way to reduce or waive the Euclid fee for specific wallets: the fee is a single per-pool rate applied uniformly to everyone, and a quote returned to an MM at request time reflects that full rate. As a result, MM strategies are uncompetitive and there is no lever to tune their edge over time.

## Solution

Introduce an **adjustable, per-wallet Euclid-fee override** that the protocol's fee admin can set for whitelisted MM wallets. The override is resolved and injected centrally at the **Router** (the trusted hub) and carried into the pool's fee calculation, so a whitelisted wallet pays a reduced — or zero — Euclid fee on its swaps. The same override is applied during swap **simulation**, so quotes returned at request time are deterministic and exactly match what execution will charge. LP fees are never affected. The override is a single explicit basis-points value per wallet (`0` = full exemption for arbitrage; small non-zero for RFQ), tunable over time by the fee admin.

## User Stories

1. As an MM arbitrage desk, I want my wallet's Euclid fee set to zero, so that my quotes improve and I keep an edge over external market makers.
2. As an MM RFQ desk, I want my wallet's Euclid fee set to a low non-zero value, so that I can compete with aggregator-backed integrators while Euclid still earns some fee.
3. As the fee admin, I want to whitelist a specific wallet with an explicit Euclid-fee value, so that I control exactly which wallets get a reduced rate and by how much.
4. As the fee admin, I want to update a wallet's override value at any time, so that I can tune an MM's edge as pool liquidity and market conditions change.
5. As the fee admin, I want to remove a wallet from the whitelist, so that it reverts to paying the normal per-pool Euclid fee.
6. As the fee admin, I want my override changes to emit an event, so that the backend can index whitelist state changes.
7. As the fee admin, I want only my role (not general or migration admin) to be able to set overrides, so that fee policy stays under fee governance.
8. As the fee admin, I want override values validated against the maximum fee, so that I cannot accidentally set an out-of-range rate.
9. As an MM wallet, I want my override to apply on every hop of a multi-hop route, so that I am not charged the full Euclid fee on intermediate pools.
10. As an MM wallet, I want my override to apply identically whether I swap from a native chain or an IBC chain, so that my exemption is consistent across chains.
11. As an MM wallet, I want my exemption keyed to my wallet on a specific chain, so that my identity is unambiguous across the cross-chain protocol.
12. As a backend integration, I want to pass the swapping wallet into the swap simulation, so that the `/routes` quote reflects the wallet's reduced Euclid fee.
13. As a backend integration, I want the simulated fee to equal the executed fee exactly, so that quotes are deterministic and never under- or over-state the real cost.
14. As a backend integration, I want to read a wallet's current override directly, so that I can audit and manage the whitelist without parsing swap simulations.
15. As a liquidity provider, I want the LP fee to be charged in full even for whitelisted wallets, so that my compensation for facilitating swaps is never subsidized away.
16. As the protocol, I want non-whitelisted swaps to behave exactly as before, so that the change is risk-free for the vast majority of traffic.
17. As the protocol, I want a wallet with no override entry to fall back to the pool's configured Euclid fee, so that the default path is unchanged.
18. As a security reviewer, I want a wallet's exemption to be unspoofable by other users on the same chain, so that only the genuine MM wallet benefits.
19. As a security reviewer, I want a chain to be unable to forge an MM wallet belonging to a different chain, so that the blast radius of a compromised chain is bounded to its own swaps.
20. As an operator, I want the new override map to start empty, so that no state migration is required and current behavior is preserved on upgrade.
21. As a developer, I want the override resolved in exactly one place for execution and reused for simulation, so that quote and execution logic cannot drift apart.
22. As a developer integrating concentrated pools later, I want a documented marker where CLP fee-tier exemption would go, so that the deferred work is not forgotten.

## Implementation Decisions

### Trust boundary & placement
- The override is resolved and injected at the **Router**, never at the per-chain Factory. The Router is the trusted hub where all swap messages converge and where the originating `CrossChainUser` is already known; the Factory is the untrusted edge.
- Native and IBC swaps both converge on the Router's single swap handler, which builds the initial swap message exactly once. The override is resolved and stamped there — one execution chokepoint.
- The existing chain-trust model is sufficient: the Router already pins a swap's sender chain to the originating channel, so a chain can only assert senders belonging to itself. No additional anti-spoofing (signatures/proofs) is added. Worst case for a fully compromised chain is under-charging the Euclid fee on its own swaps — bounded protocol-revenue loss, never a fund-safety issue, and LP fees are untouchable.

### What is overridden
- The override affects **only the Euclid fee (`euclid_fee_bps`)**. The LP fee is always charged at the pool's configured rate.
- The partner fee and the backend smart-routing fee are **out of scope** (see Out of Scope).
- The override is an explicit unsigned basis-points value. `0` means full exemption (arbitrage); a small non-zero value serves RFQ. A present entry **always wins** over the pool default (the override *is* that wallet's Euclid fee), even in the unlikely case it is set higher than the pool rate.

### Granularity & storage
- **Global per-wallet** granularity, keyed on the swapping `CrossChainUser`. Per-pool granularity is deferred (it is the future Hybrid / approach 3).
- Stored on the Router as a map keyed by the **components** of `CrossChainUser` — `(chain_uid, address)` — because `CrossChainUser` is not itself a valid storage-map primary key. Value type is the basis-points unsigned integer matching the existing Euclid-fee field type. An absent entry means "use the pool default."

### Modules

1. **Override resolution helper (Router)** — a single deep function that, given the storage and a `CrossChainUser`, returns the optional override value from the new map. Used identically by the execute and simulate paths so they cannot drift. Simple, isolated, mock-store testable.

2. **Override-aware fee calculation (pool package, CP + stable `pre_swap`)** — the pre-swap fee computation gains an optional override parameter. When present, it replaces the Euclid-fee rate; when absent, behavior is identical to today. LP-fee math is unchanged. Pure and table-driven testable.

3. **Swap message plumbing** — the router→VLP swap message gains a single optional override field at the **sender level** (not per-hop). It is forwarded hop-to-hop exactly like the sender, so it applies uniformly across every hop of a multi-hop route. CP and stable VLPs **apply** it; the concentrated VLP **forwards but does not apply** it (see CLP decision).

4. **Router management handler** — a new fee-admin-gated variant of the router's state-management message: `SetEuclidFeeOverride { user, euclid_fee_bps: Option<u64> }`. `Some(bps)` (validated against the max-fee bound) upserts the entry; `None` removes it. Emits a dedicated indexing event with attributes for the affected chain, address, and the new value (or a removal marker).

5. **Router swap injection** — at the single initial-swap construction site, resolve the override for the swap's sender via the helper and stamp it onto the outgoing swap message.

6. **Override-aware simulation** — the Router's `SimulateSwap` request gains an **optional sender**; the Router resolves the override (same helper) and threads it through the VLP simulate request into the same `pre_swap` path, so the simulated fee equals the executed fee bit-for-bit. Additionally, a `GetEuclidFeeOverride { user } -> Option<u64>` read query is exposed for backend/admin auditing. The sender field is optional for backward compatibility (absent = current behavior).

### CLP (concentrated) decision
- In concentrated pools the Euclid-fee value is **not an additive fee** — it is the protocol's *cut* of the LP swap fee. Reducing it does not improve the trader's quote; it merely shifts the protocol's share to LPs while the trader still pays the full fee-tier fee. Therefore CLP **does not apply** the override.
- CLP must still **forward** the override field to downstream hops so a CP→CLP→CP route stays exempt on its CP/stable legs.
- A documented `TODO` marks where meaningful CLP exemption (reducing the structural fee tier) would be added later. This is explicitly deferred.

### Schema compatibility & rollout (decision deferred to implementation)
- The new swap-message and simulate fields are `Option`, relying on serde's None-default for missing fields, so old/new contract combinations degrade gracefully (worst case: an MM is charged the normal fee; never an error, never a fund issue).
- The factory→router message is **unchanged**; only the Router and CP/stable VLPs change. The new override map starts empty, so **no state migration** is required beyond registering new code.
- **Open item to settle after implementation:** whether the rollout relies on graceful full-fee fallback (upgrade VLPs, then Router) or a hard version gate that rejects swaps until all contracts are upgraded. The exemption only takes effect once both the Router (injects) and the target VLPs (apply) are upgraded.

## Testing Decisions

Good tests here assert **external behavior** — the fee actually charged, the amount actually swapped, the override actually stored/removed, authorization actually enforced, and the simulated quote matching execution — not internal call shapes or private helpers. Prefer **table-driven** cases (the project convention) enumerating override states and boundaries. Integration tests use the existing rstest parameterization across Native/IBC/EVM modes and the shared reusable test helpers.

Modules to be unit/integration tested (all four selected):

1. **Override resolution helper** — unit tests over a mock store: present entry returns its value; absent entry returns none; per-chain keying (same address, different chains resolve independently); removal clears the entry.
2. **Override-aware `pre_swap` (CP + stable)** — table-driven: full exemption (`0`) zeroes the Euclid fee and increases the swapped amount accordingly; reduced bps; `None` equals the pool default; boundary at the max-fee bound; **LP fee unchanged across all cases**. Cover both CP and stable curves.
3. **`SetEuclidFeeOverride` handler** — fee-admin-only authorization (other roles and non-admins rejected); upsert vs. remove (`None`) semantics; max-fee validation rejects out-of-range values; the indexing event is emitted with correct attributes.
4. **Multi-hop + simulation integration** — exemption survives every hop of a multi-hop CP/stable route; a route through a CLP hop forwards the override but the CLP leg charges normally; a sender-aware `SimulateSwap` quote equals the fee actually charged on execution; non-whitelisted swaps are unchanged.

Prior art: existing `pre_swap`/swap unit tests in the pool package for fee math; existing router admin-gated handler tests (e.g. fee-state/release-fee updates) for the authorization + event pattern; existing multi-hop swap integration tests in `tests-integration` for the routing and simulation flows.

## Out of Scope

- **Partner fee** exemption — the partner fee is an integrator's revenue, not Euclid's, and is not waived here.
- **Backend smart-routing fee** — applied at `/routes` in the backend; the issue states it remains unchanged.
- **Concentrated-pool (CLP) Euclid exemption** — overriding the CLP protocol cut does not improve the MM's quote; meaningful CLP exemption requires reducing the structural fee tier and is deferred (marked with a documented TODO).
- **Per-pool override granularity** — deferred to the future Hybrid approach; this PRD is global per-wallet only.
- **Reward/subsidy pool** approach — explicitly deferred in the source issue in favor of deterministic, at-quote fee knowledge.
- **Reduced/zero LP fees for MMs** — out of scope; LP fees are always charged in full.

## Further Notes

- Backend coordination is required: the `/routes` and `/execute/meta-txn/*` endpoints must pass the swapping wallet into the sender-aware `SimulateSwap` to surface the reduced quote, and can use `GetEuclidFeeOverride` to manage/audit the whitelist. API readiness was flagged as a dependency in the source issue.
- Changelog: add entries under the current "in progress" section — `[router]` for the new override map, management message, event, swap injection, and sender-aware simulation/read query; `[pool]` for the override-aware `pre_swap`; `[euclid]` for the new swap-message/simulate-request fields. The new event is especially important to record for backend indexing.
- Two follow-ups are tracked as open items: (1) the rollout/version-gate decision under Schema Compatibility; (2) the deferred CLP fee-tier exemption.
