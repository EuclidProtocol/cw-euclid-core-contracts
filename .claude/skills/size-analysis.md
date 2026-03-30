---
name: size-analysis
description: Analyze WASM binary sizes for all contracts, breaking down by component (serde, contract logic, cosmwasm_std, etc.) using twiggy
---

# WASM Size Analysis

Perform a comprehensive WASM binary size analysis for the Euclid core contracts.

## Steps

### 1. Ensure twiggy is installed

Run: `which twiggy || cargo install twiggy`

### 2. Get all contract package names

Run: `grep '^name' contracts/hub/*/Cargo.toml contracts/liquidity/*/Cargo.toml contracts/hub_utilities/*/Cargo.toml contracts/common/*/Cargo.toml contracts/forwarding/*/Cargo.toml 2>/dev/null`

### 3. Build unstripped release WASMs

Run: `CARGO_PROFILE_RELEASE_STRIP="none" cargo build --release --target wasm32-unknown-unknown --lib -p factory -p router -p stable_vlp -p escrow -p lp_token -p claimer -p orderbook_deposits -p virtual_balance -p cw-multicall -p euclid-relayer -p meta-transaction -p astroport-forwarding -p osmosis-forwarding`

### 4. List optimized artifact sizes

Run: `ls -lhS artifacts/*.wasm` (if artifacts exist)

### 5. For each contract WASM in `target/wasm32-unknown-unknown/release/*.wasm`, analyze with twiggy

For each contract, run a bash script that extracts:
- Contract logic size (grep for `{contract_name}::`)
- euclid packages size (grep for `euclid::|euclid_pool::|euclid_ibc::`)
- serde ser/de size (grep for `deserialize_map|serialize::`)
- cosmwasm_std size (grep for `cosmwasm_std::`)
- cw packages size (grep for `cw20|cw2::|cw_storage_plus|cw20_base|cw_utils`)
- std/alloc/fmt size (grep for `dlmalloc|to_lowercase|base64|Display.*fmt|rustc_demangle|core::fmt`)
- .rodata size (grep for `.rodata`)
- deserialize_map monomorphization count
- Top 10 largest functions

Use `twiggy top -n 5000` and grep/awk to extract each category.

### 6. Output results

Present results as:
1. A summary table of optimized artifact sizes (sorted largest first)
2. A detailed breakdown table per contract showing component sizes
3. Top 5 largest functions per contract (for the biggest contracts)
4. Serde monomorphization counts

### 7. Optionally update SIZE_ANALYSIS.md

Ask the user if they want to update the SIZE_ANALYSIS.md file with the new results.
