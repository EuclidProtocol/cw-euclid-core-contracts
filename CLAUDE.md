# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Euclid is a cross-chain unified liquidity protocol built with CosmWasm on Cosmos. It enables token swaps, transfers, and liquidity management across multiple blockchains (Cosmos, EVM, Native) via IBC.

## Commands

### Build
```bash
# Build all contracts (Docker-based optimizer, architecture-aware)
./build.sh

# Generate JSON schemas
./build_schema.sh all        # All contracts
./build_schema.sh router     # Single contract
```

### Test
```bash
# Run all tests
cargo test

# Run tests for a specific package
cargo test -p router
cargo test -p factory

# Run a single test by name
cargo test -p router test_name
```

### Lint / Check
```bash
cargo clippy --all-targets --all-features
cargo fmt --check
```

## Architecture

### Hub-and-Spoke Model

The **Router** (`contracts/hub/router`) is the central coordinator. It lives on the Euclid hub chain. **Factory** contracts (`contracts/liquidity/factory`) live on remote chains and communicate back to the hub via IBC.

```
Remote Chain A          Hub Chain              Remote Chain B
  Factory  <--IBC-->   Router   <--IBC-->      Factory
  Escrow               VLPs (cp/stable)         Escrow
```

### Contract Layout

- `contracts/hub/` — Router, VLPs (cp_vlp, stable_vlp), virtual_balance, meta_transaction
- `contracts/hub_utilities/` — Claimer (vouchers/rewards), orderbook_deposits
- `contracts/liquidity/` — Factory, Escrow, LP token
- `contracts/common/` — euclid-relayer (IBC relay contract)
- `contracts/forwarding/` — Token forwarding contracts

### Shared Packages

- `packages/euclid/` — Core types, messages, errors. **Start here** when looking for types like `Token`, `ChainUid`, `ChainType`, `CrossChainUser`, `ContractError`
- `packages/euclid_ibc/` — IBC packet message definitions for cross-chain operations
- `packages/euclid_utils/` — Utility helpers
- `packages/pool/` — Shared pool math/logic
- `packages/mock/` — Mock implementations for testing

### Cross-Chain Message Flow

1. User calls `ExecuteMsg` on Router (e.g., swap, transfer, add liquidity)
2. Router creates an IBC packet (`SendPacket`) to the target chain's Factory
3. Factory executes the operation (escrow, VLP interaction)
4. Factory sends acknowledgment back
5. Router processes `AcknowledgePacket` — success path releases vouchers or triggers callbacks; timeout path reverts

### Key State in Router (`contracts/hub/router/src/state.rs`)

Storage items/maps:
- `VLPS: Map<(String, String), Addr>` — registered VLP pool addresses (keyed by token pair)
- `TOKEN_VLPS: Map<Token, Vec<Addr>>` — all VLPs associated with a given token
- `PENDING_SWAPS: Map<String, RouterCrossChainSwapExecuteMsg>` — in-flight cross-chain swaps (keyed by tx_id)
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

### Testing

- Unit tests: `#[cfg(test)]` blocks within each contract
- Integration tests: `tests-integration/src/` using `cw-orch` and `cw-orch-interchain`
- Fuzz tests: `tests-fuzz/`

The `tests-integration` package includes all contracts as dev-dependencies and sets up multi-contract and multi-chain scenarios.

When writing unit tests for a contract, use the `unit-test-writer` agent. It understands the project's test conventions (rstest parameterization, `MockDeps` fixtures, `init` helpers, state assertions). Invoke it via the `/write-tests <contract-path>` skill.

### Changelog

The project maintains a `CHANGELOG.md` following [Keep a Changelog](https://keepachangelog.com/) format. Each release is named after a star with a status (in progress, freezed, released). When making contract or package changes (not test only), add an entry under the current "in progress" section in the appropriate category (Added, Changed, Fixed, Deprecated, Removed, Security). Prefix entries with the contract or package name in brackets, e.g. `[router]`, `[euclid]`. One line per logical change. Event changes are especially important to track as they affect backend indexing.
