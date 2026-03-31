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

- **Factory** (`contracts/liquidity/factory/`) — User-facing entry point on each chain. Handles pool creation requests, swaps, liquidity operations. Sends `RouterCrossChainExecuteMsg` via IBC to router; receives `FactoryCrossChainExecuteMsg` back as ACKs.
- **Escrow** (`contracts/liquidity/escrow/`) — Holds real tokens per chain.
- **lp_token** (`contracts/liquidity/lp_token/`) — CW20 LP tokens for CP/Stable pools.
- **position_token** (`contracts/liquidity/position_token/`) — NFT-style tokens for concentrated liquidity positions.

### Cross-Chain Message Flow

```
User → Factory (chain) →[IBC]→ Router (VSL) → VLP contract
                         ←[ACK]←
```

- IBC messages: `RouterCrossChainExecuteMsg` (packages/euclid_ibc/src/router_ibc.rs)
- ACK messages: `FactoryCrossChainExecuteMsg` (packages/euclid_ibc/src/factory_ibc.rs)
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
- **forwarding** — DEX forwarding interfaces (Astroport, Osmosis)
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
