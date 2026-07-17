# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is this?

Euclid Protocol — a cross-chain unified liquidity DEX built on CosmWasm. Hub-and-spoke architecture where a **Virtual Settlement Layer (VSL)** coordinates liquidity across Cosmos, EVM, and native chains. Users interact with per-chain **Factory** contracts; all virtual liquidity lives on the hub via **Router** → **VLP** contracts.

## Build, Test, Lint Commands

```bash
# Build
cargo wasm                              # Single contract WASM build (per-contract dir)
./build.sh all                          # Docker optimized build (all contracts → ./artifacts/)

# Test
cargo test                              # All tests (workspace)
cargo test --lib                        # Unit tests only
cargo test -p tests-integration         # Integration tests only
cargo test -p tests-integration <name>  # Single integration test (e.g. concentrated_v3_swap)
cargo test -p concentrated_vlp --lib    # Unit tests for a single contract

# Lint
cargo fmt --all -- --check              # Format check
cargo fmt --all                         # Format fix
cargo clippy -- -W clippy::pedantic     # Clippy (matches CI)

# Schema generation (per-contract dir)
cargo schema
```

CI runs: `cargo unit-test --locked`, `cargo wasm --locked`, format check, clippy pedantic.

## Architecture

### Hub Contracts (deployed on VSL)

- **Router** (`contracts/hub/router/`) — Central dispatcher. Receives all cross-chain messages from factories. Manages pool registry (`VLPS` map for CP/Stable, `CONCENTRATED_VLPS` map for CLP). Dispatches to the correct VLP. Tracks escrow balances, token denoms, chain registry.
- **cp_vlp** (`contracts/hub/cp_vlp/`) — Constant product AMM (x*y=k).
- **stable_vlp** (`contracts/hub/stable_vlp/`) — Stable swap curve.
- **concentrated_vlp** (`contracts/hub/concentrated_vlp/`) — Uniswap V3-style concentrated liquidity with tick-based positions, fee growth accounting, and oracle observations.
- **virtual_balance** (`contracts/hub/virtual_balance/`) — Voucher ledger. No real assets move on the hub; this tracks who owns what across chains. Only router can call it.
- **meta_transaction** (`contracts/hub/meta_transaction/`) — Meta-tx relay support.

### Liquidity Contracts (deployed per-chain)

- **Factory** (`contracts/liquidity/factory/`) — User-facing entry point on each chain. Handles pool creation requests, swaps, liquidity operations. Sends the `RouterReceiveMsg` wire envelope via IBC to router; receives the `FactoryReceiveMsg` wire envelope back as ACKs.
- **Escrow** (`contracts/liquidity/escrow/`) — Holds real tokens per chain.
- **lp_token** (`contracts/liquidity/lp_token/`) — CW20 LP tokens for CP/Stable pools.
- **position_token** (`contracts/liquidity/position_token/`) — NFT-style tokens for concentrated liquidity positions.

### Cross-Chain Message Flow

```
User → Factory (chain) →[IBC]→ Router (VSL) → VLP contract
                         ←[ACK]←
```

1. User calls `ExecuteMsg` on Router (e.g., swap, transfer, add liquidity)
2. Router creates an IBC packet (`SendPacket`) to the target chain's Factory
3. Factory executes the operation (escrow, VLP interaction)
4. Factory sends acknowledgment back
5. Router processes `AcknowledgePacket` — success path releases vouchers or triggers callbacks; timeout path reverts

### Key State in Router (`contracts/hub/router/src/state.rs`)

Storage items/maps:
- `VLPS: Map<(String, String), Addr>` — registered VLP pool addresses (keyed by token pair)
- `TOKEN_VLPS: Map<Token, Vec<Addr>>` — all VLPs associated with a given token
- `PENDING_SWAPS: Map<String, SwapSendMsg>` — in-flight cross-chain swaps (keyed by tx_id)
- `PENDING_RELEASE_VOUCHER: Map<String, PendingReleaseVoucher>` — in-flight voucher releases awaiting IBC ack
- `ESCROW_BALANCES: Map<(String, ChainUid), Uint128>` — **DEPRECATED** (moved to virtual_balance contract)
- `LOCKED_CHAINS: Item<Vec<ChainUid>>` — chains paused for emergency stops
- `DEFAULT_RELEASE_FEE: Item<Uint128>` — fallback release fee when no per-chain fee is set
- `RELEASE_FEES: Map<(Token, ChainUid), Uint128>` — per-(token, chain) release fee overrides
- `CHAIN_TIMEOUT_SECONDS: Map<ChainUid, u64>` — per-chain IBC packet timeout in seconds

### IBC Module (`contracts/hub/router/src/ibc/`)

- `channel.rs` — channel open/close handshake
- `receive/` — handlers for incoming packets: `swap.rs`, `token.rs`, `pool.rs`, `base.rs`
- `ack_and_timeout.rs` — success/failure callbacks per packet type

### Reply Pattern

Contracts use CosmWasm's `SubMsg` + `reply` for async outcomes (VLP instantiation, liquidity ops, cross-chain callbacks). Reply IDs are defined as constants at the top of `contract.rs` or `reply.rs`.

### Admin Pattern

```rust
pub struct EuclidAdmin {
    general_admin: Addr,   // day-to-day operations
    fee_admin: Addr, // fee management
    migration_admin: Addr, // contract migrations
}
```

- IBC messages: `RouterReceiveMsg` (packages/euclid_ibc/src/wire/envelope/router.rs)
- ACK messages: `FactoryReceiveMsg` (packages/euclid_ibc/src/wire/envelope/factory.rs)
- Native chains skip IBC, use `NativeReceiveCallback` directly
- All async operations use **SubMsg reply pattern** (router/src/reply.rs) with defined reply IDs

### Key Types (packages/euclid/src/)

- **Token** (`token.rs`) — Newtype over String. Validated ASCII alphanumeric + '.', max 64 chars. Universal token identifier.
- **Pair** (`token.rs`) — Always canonically ordered (lexicographic). `Pair::new()` enforces ordering.
- **CrossChainUser** (`cross_chain_user.rs`) — `{ chain_uid, address }`. Universal principal across all chains.
- **ChainUid** (`chain.rs`) — Newtype identifier for chains. Lowercase alphanumeric + '.'.
- **PoolKey** (`msgs/vlp/base.rs`) — Identifies concentrated pools: `{ pair, pool_type: Concentrated { fee_tier_bps, tick_spacing } }`.

### Packages

- **euclid** — Core types, all message definitions, error types
- **pool** — AMM math (constant product, stable swap calculations)
- **euclid_ibc** — IBC message enums for router↔factory communication
- **euclid_utils** — Shared utilities
- **relayer** — Multi-sig verification for meta-tx relay
- **mock** — Test mocking utilities

### Test Infrastructure (tests-integration/)

- **cw-orch** 0.28.0 for contract deployment interfaces. Messages derive `#[cw_orch::ExecuteFns]` and `#[cw_orch::QueryMsgFns]` for typed calling.
- **rstest** for parameterized tests across 3 chain modes: Native, IBC, EVM. Single test function runs all modes.
- **tests_reusable/** — Shared test logic (parameterized by mode). **tests/** — Test entry points that call into reusable modules.
- **helpers/** — Chain setup (`setup_router`, `setup_factory`), token operations, pool creation helpers.

## Coding Conventions

- **Test-driven development** — write tests first, then implement. Prefer **table-driven tests** (array of test cases iterated in a loop) for comprehensive coverage of edge cases and input variations.
- **No `.unwrap()` in smart contracts** — use `?`, `.ok_or_else()`, or `unwrap_or_default()`.
- Each contract follows the structure: `contract.rs` (entry points), `execute/` (handlers), `query.rs`, `state.rs`, `reply.rs` (if async), `interface.rs` (cw-orch wrapper), `mock.rs`.
- Execute functions belong in `execute/` or `execute.rs`, not in `contract.rs`.
- Non-wasm dependencies (cw-orch, cw-multi-test, mock) are gated behind `cfg(not(target_arch = "wasm32"))`.
- All contracts use `library` feature flag to disable entry point exports when used as a dependency.
- Pool math uses `Uint512`/`Uint256` for precision. Concentrated VLP uses `2^96` and `2^128` fixed-point scaling.

When writing unit tests for a contract, use the `unit-test-writer` agent. It understands the project's test conventions (rstest parameterization, `MockDeps` fixtures, `init` helpers, state assertions). Invoke it via the `/write-tests <contract-path>` skill.

### Changelog

The project maintains a `CHANGELOG.md` following [Keep a Changelog](https://keepachangelog.com/) format. Each release is named after a star with a status (in progress, freezed, released). When making contract or package changes (not test only), add an entry under the current "in progress" section in the appropriate category (Added, Changed, Fixed, Deprecated, Removed, Security). Prefix entries with the contract or package name in brackets, e.g. `[router]`, `[euclid]`. One line per logical change. Event changes are especially important to track as they affect backend indexing.
