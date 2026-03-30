# Developer Guide

## Contract Interface Pattern

Cross-contract `to_json_binary` calls that reference a full `ExecuteMsg` enum pull in serde monomorphization code for ALL variants, even when only one variant is needed. This is the #1 contributor to WASM binary size.

The solution: single-variant "interface" enums that serialize identically to the parent enum variant but only generate serde code for that one variant.

## How It Works

Each interface type is a single-variant `#[cw_serde]` enum that implements the `ContractInterface` trait (defined in `packages/euclid/src/interface.rs`). The trait provides `into_cosmos_msg()` and `into_cosmos_msg_with_funds()` helpers.

Before:
```rust
let msg = CosmosMsg::Wasm(WasmMsg::Execute {
    contract_addr: escrow_address.into_string(),
    msg: to_json_binary(&escrow::ExecuteMsg::AddAllowedDenom {
        denom: token.token_type.clone(),
    })?,
    funds: vec![],
});
```

After:
```rust
let msg = escrow::interface::AddAllowedDenomMsg::AddAllowedDenom {
    denom: token.token_type.clone(),
}
.into_cosmos_msg(escrow_address)?;
```

## Rules

1. Never use a full `ExecuteMsg` of another contract in `to_json_binary`. Always use the interface type.
2. Self-calls (contract calling itself) should also use interface types.
3. Every interface type must have a serialization equivalence test.

## Creating a New Interface

Step by step:

1. Add a new single-variant enum in the target contract's `interface.rs` file (in `packages/euclid/src/msgs/{contract}/interface.rs`).
2. Add `#[cw_serde]` derive and `impl ContractInterface for YourMsg {}`.
3. Add a doc comment referencing the parent enum variant: `/// Interface for [super::msg::ExecuteMsg::YourVariant]`.
4. Add a test using `assert_interface_eq` to verify serialization equivalence.
5. Fields and variant name must match the parent enum exactly.

## File Conventions

| Convention | Value |
|---|---|
| Interface file location | `packages/euclid/src/msgs/{contract}/interface.rs` |
| Naming | `{VariantName}Msg` (e.g., `MintMsg`, `WithdrawMsg`) |
| Trait | All implement `ContractInterface` from `euclid::interface` |
| Tests | One test per interface type using `assert_interface_eq` |

## Size Analysis

To analyze WASM binary sizes:

1. Install twiggy: `cargo install twiggy`
2. Build unstripped WASMs: `CARGO_PROFILE_RELEASE_STRIP="none" cargo build --release --target wasm32-unknown-unknown --lib`
3. Analyze: `twiggy top -n 30 target/wasm32-unknown-unknown/release/{contract}.wasm`
4. Check optimized sizes: `ls -lhS artifacts/*.wasm`

Or use the `/size-analysis` Claude Code skill for automated analysis.
