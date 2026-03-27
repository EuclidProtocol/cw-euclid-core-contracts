---
name: cosmwasm-audit
description: Security audit and code standards review for Euclid Protocol CosmWasm smart contracts. Use when reviewing PRs, auditing contracts, or checking code quality in this monorepo.
metadata:
  filePattern:
    - "contracts/**/*.rs"
    - "packages/**/*.rs"
    - "tests-integration/**/*.rs"
    - "tests-fuzz/**/*.rs"
  bashPattern:
    - "cargo (test|check|clippy|build|fmt)"
    - "audit"
---

# CosmWasm Contract Audit — Euclid Protocol

## When to Use

- Reviewing a PR that touches contract or package code
- Performing a security audit on new or changed contracts
- Checking code quality and Rust standards compliance
- Verifying test coverage for contract changes
- Pre-deployment readiness review

## Project Architecture

```
contracts/
├── hub/                         # Hub chain (VSL) contracts
│   ├── router/                  # Cross-chain routing, entry point for all hub ops
│   ├── cp_vlp/                  # Constant product virtual liquidity pool
│   ├── stable_vlp/              # Stable swap virtual liquidity pool
│   ├── concentrated_vlp/        # Concentrated liquidity pool (Uni V3-style)
│   ├── virtual_balance/         # Voucher ledger (no real tokens on hub)
│   └── meta_transaction/        # Meta-transaction support
├── liquidity/                   # User chain contracts
│   ├── factory/                 # Pool creation, IBC relay to router
│   ├── escrow/                  # Holds real tokens on user chains
│   ├── lp_token/                # CW20 LP token
│   └── position_token/          # CW721 NFT for CLP positions
├── forwarding/                  # DEX-specific forwarding
│   ├── osmosis-forwarding/
│   └── astroport-forwarding/
├── common/
│   ├── cw-multicall/
│   └── euclid-relayer/
└── hub_utilities/
    ├── claimer/
    └── orderbook_deposits/

packages/                        # Shared workspace crates
├── euclid/                      # Core types, messages, errors
│   └── src/msgs/                # All contract message definitions
├── euclid_utils/                # Shared utilities
├── pool/                        # Pool math (CP, stable, concentrated)
├── forwarding/                  # Forwarding abstractions
├── relayer/                     # Relayer types
└── mock/                        # Test mocks

tests-integration/               # Multi-contract integration tests (cw-multi-test)
tests-fuzz/                      # Fuzz tests with invariant checking
```

### Trust Boundaries

- **Router is the gatekeeper**: Only the router can call VLP execute functions
- **Factory initiates via IBC**: Factory -> relay -> Router -> VLP
- **Virtual balance is a voucher system**: No real tokens on the hub chain
- **Escrow holds real assets**: On user chains, guarded by factory
- **Position ownership**: CW721 NFT (factory-side) + `position.owner` (CLP-side)

### Key Dependencies

- `cosmwasm-std` 2.2.2, `cw-storage-plus` 2.0.0, `cw2` 2.0.0
- `cw-multi-test` 2.4.0 for integration tests
- `cw-orch` 0.28.0 for deployment orchestration
- Docker-based optimizer (`cosmwasm/optimizer:0.17.0`) for production builds

## Audit Checklist

### 1. Access Control

- [ ] All hub contract execute functions check `info.sender == state.router`
- [ ] Factory operations verify caller is authorized (admin or relayer)
- [ ] Admin-only functions (config updates, migrations) have explicit `is_admin` guards
- [ ] No execute path allows arbitrary external callers to bypass the router
- [ ] IBC receive handlers validate the source chain/channel

### 2. Math & Overflow Safety

- [ ] All token arithmetic uses `Uint128`/`Uint256` checked operations
- [ ] No raw `u128` arithmetic that could overflow
- [ ] Rounding directions protect the protocol (round against the user)
- [ ] Pool math: verify division-by-zero guards on empty pools
- [ ] Concentrated liquidity: Q96/Q128 fixed-point uses Uint256/Uint512 intermediates
- [ ] Fee calculations cannot underflow or produce negative amounts
- [ ] Slippage bounds are enforced before executing swaps
- [ ] If porting from a reference (e.g., Uniswap V3), verify behavioral fidelity — are any valid use cases blocked by overly strict validation?

### 3. State Consistency

- [ ] No partial state updates: if a multi-step operation fails mid-way, is state rolled back?
- [ ] CosmWasm atomicity relied upon correctly (submessage replies handle errors)
- [ ] Cross-contract state (factory ↔ router ↔ VLP) stays consistent on IBC timeout/error
- [ ] IBC acknowledgement and timeout handlers properly revert state
- [ ] Position state (CLP) matches NFT state (factory) — no orphaned positions
- [ ] IBC temporal divergence: are there windows where factory-side and hub-side state are inconsistent? If so, no authorization logic should depend on the stale side

### 4. Fund Safety

- [ ] `BankMsg::Send` only sends to intended recipients
- [ ] Escrow withdrawals match expected amounts (no drain vectors)
- [ ] Virtual balance credits/debits are symmetric
- [ ] No path allows minting unbacked virtual balances
- [ ] LP token supply matches actual liquidity deposited
- [ ] Fee collection cannot be manipulated by sequencing
- [ ] Reserve/balance accounting invariant: reserves track all token movements (credits on deposit/swap-in, debits on withdrawal/swap-out/fee-collection)

### 5. Input Validation

- [ ] All `ExecuteMsg` variants validate parameters (non-zero amounts, valid denoms)
- [ ] `InstantiateMsg` validates initial config (admin, router, allowed denoms)
- [ ] String inputs are bounded (denom lengths, chain IDs)
- [ ] Pagination parameters have upper bounds to prevent DoS
- [ ] Duplicate detection where needed (e.g., duplicate token pairs)

### 6. Storage Access Patterns

- [ ] Use `.may_load()?.unwrap_or_default()` instead of `.load()` when the key may not exist — bare `.load()` returns `StdError::NotFound` which produces unhelpful errors and can block operations unexpectedly
- [ ] No unbounded iteration over storage (maps, indexes) — check `execute`, `query`, AND `migrate` paths
- [ ] Pagination used for all list queries
- [ ] No user-controllable loop bounds in execute paths
- [ ] Storage keys are bounded in size
- [ ] IBC packet handling has bounded computation

### 7. Submessage Dispatch & Replies

- [ ] `Reply` entry points validate `msg.id` matches expected submessage
- [ ] State is not left in an inconsistent pre-reply state
- [ ] Submessage failures are handled (ReplyOn::Error paths)
- [ ] No assumption that submessage execution order is sequential when it isn't
- [ ] Operations that need rollback on failure use `SubMsg::reply_always`, NOT `add_message` — `add_message` has no reply handler, so if the dispatched call fails the parent state is already committed
- [ ] When multiple message types share a reply ID, the reply handler correctly disambiguates (e.g., by trying to deserialize different response types)

### 8. Migration Safety

- [ ] `MigrateMsg` handler exists for contracts that need upgradeability
- [ ] State type changes include migration logic
- [ ] `cw2::set_contract_version` called on instantiate and migrate
- [ ] Old state can deserialize into new types (or explicit migration path exists)
- [ ] Migration tested with realistic pre-migration state
- [ ] **Migration gas limits**: `migrate()` must not load all entries from an unbounded map into memory — use pagination or streaming. A permanently unmigrateable contract is worse than a reverted transaction
- [ ] Migration handles pre-existing uncollected fees/rewards — document whether they are preserved, zeroed (assigned to protocol), or lost

### 9. Code Quality (Rust)

- [ ] No `.unwrap()` in non-test code (use `?` or explicit error)
- [ ] No `unsafe` blocks in contract code
- [ ] Custom error types via `thiserror` (not bare `StdError::generic_err`)
- [ ] No unnecessary `.clone()` (use references where possible)
- [ ] `cargo clippy --all-targets` passes clean
- [ ] `cargo fmt --check` passes
- [ ] Dead code removed (no `#[allow(dead_code)]` in production)
- [ ] No test-only functionality in production message types — `test_fail`, debug flags, etc. should be gated behind `#[cfg(test)]` or removed
- [ ] No unused state fields (vestigial items from other pool types, unimplemented guards, etc.)

### 10. Cross-Contract Consistency

- [ ] Shared logic (key encoding, validation, type conversion) lives in a shared package, not duplicated across contracts — duplicated implementations can silently diverge
- [ ] Reply ID constants are unique across the contract or handlers correctly disambiguate shared IDs
- [ ] Pending-request maps (e.g., `PENDING_SWAPS`, `PENDING_REMOVE_LIQUIDITY`) are keyed to prevent collision and cleaned up on ack/timeout

### 11. Test Coverage

- [ ] Every `ExecuteMsg` variant has at least one happy-path test
- [ ] Every `ExecuteMsg` variant has at least one error-path test
- [ ] Every `QueryMsg` variant is tested
- [ ] Integration tests cover multi-contract flows (factory -> router -> VLP)
- [ ] IBC flows tested (send, ack, timeout)
- [ ] Fuzz tests exist for math-heavy logic (pool math, concentrated liquidity)
- [ ] Fuzz invariants check: total supply consistency, balance conservation, no negative amounts
- [ ] Edge cases: zero amounts, max amounts, empty pools, single-sided liquidity

## PR Audit Process

When auditing a PR, follow this order:

### Step 1: Scope the changes
```bash
# Get changed files — use the PR's base branch (usually development)
git diff development...HEAD --name-only | grep -E '\.(rs|toml)$'

# Summarize the diff size
git diff development...HEAD --stat

# Per-directory breakdown for large PRs
git diff development...HEAD --stat -- contracts/hub/
git diff development...HEAD --stat -- contracts/liquidity/
git diff development...HEAD --stat -- packages/
git diff development...HEAD --stat -- tests-integration/
```

Categorize files by risk: new contracts > execute handlers > state changes > math > queries > tests. This determines reading order.

### Step 2: Check build health (run in background)

Start `cargo clippy --all-targets` in the background while you read code. Don't block on it.

```bash
cargo clippy --all-targets 2>&1 | tail -60   # run in background
cargo fmt --check                              # fast, run immediately
```

### Step 3: Review changed code against checklist

Read in this order for maximum signal:
1. **State definitions** (`state.rs`) — understand the data model first
2. **Execute handlers** (`contract.rs`, `execute/*.rs`) — business logic, access control
3. **Math modules** — correctness, overflow, rounding
4. **Migration** (`migrate.rs`) — state transitions, gas limits, data loss
5. **Cross-contract integration** (router, factory changes) — consistency, reply chains
6. **Reply handlers** — error handling, data flow
7. **Queries** — pagination bounds, no state mutation

Apply relevant checklist sections based on what was touched. Check blast radius: did changes to packages/ affect other contracts?

#### Parallelizing with sub-agents (for large PRs)

For PRs touching 50+ files or 5k+ lines, split the review across parallel agents. Each agent gets the full checklist but a scoped file set:

```
Agent 1 (Critical — core engine):
  - contracts/hub/concentrated_vlp/src/{contract,state,migrate}.rs
  - contracts/hub/concentrated_vlp/src/math/*.rs
  - Focus: Math correctness, state consistency, migration safety, gas limits

Agent 2 (High — cross-contract integration):
  - contracts/hub/router/src/**/*.rs (changed files)
  - contracts/liquidity/factory/src/**/*.rs (changed files)
  - Focus: Access control, IBC flow, reply chains, pending request lifecycle

Agent 3 (Medium — types & messages):
  - packages/euclid/src/**/*.rs (changed files)
  - packages/euclid_ibc/src/**/*.rs (changed files)
  - Focus: Message type validation, test-only fields, cross-contract type consistency

Agent 4 (Background — build & coverage):
  - Run clippy, fmt, tests
  - Grep for .unwrap() in non-test code
  - Count test coverage per ExecuteMsg variant
  - Focus: Code quality checklist items
```

Each agent should return structured findings in `H-XX/M-XX/L-XX/I-XX` format. The main agent deduplicates and merges into the final report. Agents 1 and 2 should run in foreground (findings inform each other); Agents 3 and 4 can run in background.

### Step 4: Verify test coverage for the PR
- Does every new execute path have a test?
- Are error conditions tested?
- If math changed, are fuzz tests updated?

### Step 5: Check migration compatibility
- If stored state types changed, is there a migration handler?
- Run `cargo schema` if message types changed

### Step 6: Write findings report

Write the report to a markdown file in the repo root. Naming convention:

- **PR audit**: `AUDIT_PR<number>.md` (e.g., `AUDIT_PR135.md`)
- **Ad-hoc audit** (no PR number): `AUDIT_<DDMMYY>.md` (e.g., `AUDIT_270326.md`)

If a file for this PR/date already exists, append new findings and update the summary counts rather than overwriting.

Use the severity-prefixed format established in `CLP_AUDIT_FINDINGS.md`:
- `H-XX` for High, `M-XX` for Medium, `L-XX` for Low, `I-XX` for Informational

## Findings Report Template

Use this format for each finding:

```markdown
### H-XX: Title

| | |
|-|-|
| **Severity** | Critical / High / Medium / Low / Informational |
| **Location** | `path/to/file.rs:LINE` |
| **Status** | Open / Acknowledged / Fixed |

**Description:**
What the issue is.

**Exploit scenario** (security findings only):
Step-by-step how this could be exploited.

**Recommendation:**
Specific fix.
```

### Severity Definitions (Euclid-specific)

| Severity | Definition |
|----------|-----------|
| **Critical** | Immediate fund loss or unauthorized access through external user actions |
| **High** | Code defect causing operational failure or fund lock under reachable conditions |
| **Medium** | Defense-in-depth violation, design concern, or issue requiring specific preconditions |
| **Low** | Code quality, gas optimization, or minor robustness improvement |
| **Informational** | Style, documentation, or suggestion with no security impact |

## Build & Test Commands

```bash
# Full workspace check
cargo clippy --all-targets

# Format check
cargo fmt --check

# Unit tests (all workspace)
cargo test --workspace

# Integration tests only
cargo test -p tests-integration

# Fuzz tests (compile check)
cargo test -p tests-fuzz --no-run

# Production build (Docker optimizer)
./build.sh

# Generate schemas
./build_schema.sh
```

## Known Patterns in This Codebase

- **Router-only access**: Hub contracts use `ensure!(info.sender == state.router, ...)`
- **IBC flow**: Factory -> IBC relay -> Router -> VLP -> reply back through IBC ack
- **Virtual balance**: Credits/debits tracked in `virtual_balance` contract, no real token movement on hub
- **Pool types**: CP (constant product), Stable (StableSwap curve), Concentrated (Uni V3-style ticks)
- **Escrow pattern**: Real tokens locked in escrow on user chains, virtual balances on hub
- **Position NFTs**: CLP positions represented as CW721 tokens on user chain, metadata tracked in factory
- **CW2 versioning**: All contracts set version on instantiate for migration safety
- **Pool key encoding**: `pool_key_to_map_key()` uses null-byte delimiters — must be identical in router and factory (currently duplicated, should be in `euclid` package)
- **Wrapping arithmetic for fee growth**: CLP fee accumulators use `wrapping_add`/`wrapping_sub` (mod 2^256) — this is correct V3 behavior, not a bug. Only deltas matter.

## Common CosmWasm Anti-Patterns to Watch For

These produced real findings in prior audits of this codebase:

| Anti-pattern | Example | Fix |
|---|---|---|
| `.load()` on optional key | `CHAIN_LP_TOKENS.load(storage, chain_uid)?` fails if chain not registered | `.may_load()?.unwrap_or_default()` |
| `add_message` for fallible ops | NFT mint via `add_message` — no rollback if mint fails | `SubMsg::reply_always` with error handler |
| Unbounded `.collect()` in migrate | `POSITIONS.range(...).collect::<Vec<_>>()` — OOM on large maps | Paginated migration or streaming |
| Test flags in production types | `test_fail: Option<bool>` in `VlpSwapMsg` | `#[cfg(test)]` gate or remove |
| Duplicated cross-contract logic | `pool_key_to_map_key()` in both router and factory | Move to shared package |
| Unused state fields | `Slot0.unlocked` defined but never checked | Implement or remove |

## Reference

- Audit findings: `CLP_AUDIT_FINDINGS.md` (concentrated_vlp, 2026-03-27)
- Plans: `docs/plans/`
- TODOs: `TODOs.md`
