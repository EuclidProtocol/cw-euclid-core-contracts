# WASM Size Analysis

Analysis date: 2026-03-31
Branch: `fix/contract-sizes`
Tooling: `twiggy v0.8.0` on unstripped release builds, optimized artifacts via `cosmwasm/optimizer`

## Contract Sizes (Optimized Artifacts)

| Contract              | Size (KB) | % of Total |
|-----------------------|-----------|------------|
| factory               | 943       | 18.6%      |
| router                | 861       | 17.0%      |
| stable_vlp            | 414       | 8.2%       |
| lp_token              | 341       | 6.7%       |
| orderbook_deposits    | 338       | 6.7%       |
| claimer               | 312       | 6.2%       |
| meta_transaction      | 309       | 6.1%       |
| virtual_balance       | 309       | 6.1%       |
| euclid_relayer        | 264       | 5.2%       |
| escrow                | 264       | 5.2%       |
| astroport_forwarding  | 245       | 4.8%       |
| osmosis_forwarding    | 241       | 4.8%       |
| cw_multicall          | 231       | 4.6%       |
| **Total**             | **5,072** | **100%**   |

Factory and router together account for **35.6%** of total binary size.

## Size Breakdown by Component

Measured from unstripped release builds. Debug sections excluded. Values are approximate because
symbol attribution in optimized WASM is not exact.

### Factory (943 KB optimized)

| Component        | Size (KB) | Notes                                    |
|------------------|-----------|------------------------------------------|
| serde ser/de     | 304.7     | 113 `deserialize_map` monomorphizations  |
| Contract logic   | 130.7     | `reusable_internal_ack_call` alone is 30 KB |
| euclid packages  | 85.3      | euclid, euclid_ibc                       |
| std/alloc/fmt    | 81.5      | dlmalloc, base64, Display, rustc_demangle |
| cosmwasm_std     | 71.8      |                                          |
| .rodata          | 62.1      | String literals, error messages          |
| cw packages      | 46.8      | cw20, cw20_base, cw2, cw_storage_plus   |

### Router (861 KB optimized)

| Component        | Size (KB) | Notes                                    |
|------------------|-----------|------------------------------------------|
| serde ser/de     | 268.4     | 108 `deserialize_map` monomorphizations  |
| Contract logic   | 132.7     |                                          |
| euclid packages  | 83.0      |                                          |
| std/alloc/fmt    | 82.4      |                                          |
| cosmwasm_std     | 74.3      |                                          |
| .rodata          | 64.4      |                                          |
| cw packages      | 39.7      |                                          |

### Stable VLP (414 KB optimized)

| Component        | Size (KB) | Notes                                    |
|------------------|-----------|------------------------------------------|
| serde ser/de     | 117.3     | 50 monomorphizations                     |
| std/alloc/fmt    | 75.6      |                                          |
| euclid packages  | 69.6      | euclid, euclid_pool                      |
| cosmwasm_std     | 59.5      |                                          |
| .rodata          | 40.2      |                                          |
| Contract logic   | 18.1      |                                          |
| cw packages      | 11.6      |                                          |

### Smaller Contracts (231 to 341 KB)

| Contract             | Serde (KB) | Contract Logic (KB) | cosmwasm_std (KB) | cw packages (KB) |
|----------------------|------------|---------------------|-------------------|-------------------|
| lp_token             | 86.6       | 11.9                | 39.8              | 54.2              |
| orderbook_deposits   | 92.1       | 40.2                | 36.9              | 10.3              |
| claimer              | 85.5       | 19.0                | 34.9              | 8.7               |
| meta_transaction     | 64.9       | 15.1                | 40.9              | 5.2               |
| virtual_balance      | 67.5       | 29.8                | 38.2              | 15.0              |
| euclid_relayer       | 54.0       | 13.0                | 34.0              | 8.7               |
| escrow               | 62.5       | 18.0                | 36.9              | 11.3              |
| astroport_forwarding | 72.2       | 8.8                 | 44.4              | 10.7              |
| osmosis_forwarding   | 68.4       | 9.2                 | 45.4              | 10.2              |
| cw_multicall         | 66.6       | 1.0                 | 41.0              | 0.9               |

## Biggest Size Contributors (All Contracts)

### 1. Serde Serialization/Deserialization

**The single largest contributor across all contracts.** In factory it's 304.7 KB (over 30% of the binary). Each unique message type generates its own monomorphized `deserialize_map` and `serialize` functions. Factory has 113 of these, router has 108.

The `euclid::msgs` module defines many large enums (`ExecuteMsg`, `QueryMsg`) with deeply nested variants. Each variant type generates its own serde code. The `ExecuteMsg::serialize` function for factory alone is 4.6 KB.

### 2. Shared Runtime Overhead (~70 to 82 KB per contract)

Every contract pays a fixed cost for `std/alloc/fmt`:
- `dlmalloc` (~5 KB) for heap allocation
- `base64` encode/decode (~7 KB)
- `rustc_demangle` (~6 KB) for panic backtraces
- `to_lowercase` (~4 KB) from `alloc::str`
- Various `core::fmt` and `Display` implementations

This is an unavoidable baseline of ~70 KB per contract.

### 3. Contract Logic (Especially Factory/Router)

Factory's `reusable_internal_ack_call` is 30 KB alone. This is the single largest contract function in the entire codebase. Router's largest functions are more evenly distributed (7 KB max).

### 4. Duplicate Dependencies

The workspace has significant version duplication that inflates the forwarding contracts:

| Crate            | Versions         |
|------------------|------------------|
| cosmwasm-std     | 1.5.11, 2.2.2   |
| cw-storage-plus  | 1.2.0, 2.0.0    |
| cw2              | 1.1.2, 2.0.0    |
| cw20             | 1.1.2, 2.0.0    |
| prost            | 0.11.9, 0.13.5  |
| itertools        | 0.10, 0.12, 0.13, 0.14 |
| base64           | 0.21.7, 0.22.1  |
| sha2             | 0.9.9, 0.10.8   |

The v1 duplicates come from the `astroport` dependency pulling in older cosmwasm/cw ecosystem crates.

### 5. .rodata (String Literals)

Factory carries 62 KB of string data. This includes error messages, JSON field names, and IBC channel strings. Router has 64 KB.

## Top Functions by Size

### Factory

| Function                                     | Size (KB) |
|----------------------------------------------|-----------|
| `ibc::ack_and_timeout::reusable_internal_ack_call` | 29.7      |
| `execute::swap::execute_swap_request`        | 5.5       |
| `execute::pool::execute_request_pool_creation` | 5.3      |
| `contract::execute` (dispatch)               | 4.7       |
| `execute::pool::add_liquidity_request`       | 4.3       |
| `execute::relay::execute_send_packet`        | 4.1       |
| `ibc::receive::reusable_internal_call`       | 3.9       |
| `execute::relay::execute_receive_acknowledgement` | 3.9  |
| `execute::token::execute_deposit_token`      | 3.8       |

### Router

| Function                                     | Size (KB) |
|----------------------------------------------|-----------|
| `execute::base::execute_manage_router_state` | 7.2       |
| `ibc::receive::pool::ibc_execute_request_pool_creation` | 6.0 |
| `ibc::receive::swap::ibc_execute_swap`       | 6.0       |
| `ibc::receive::base::reusable_internal_call` | 5.2       |
| `execute::token::execute_transfer_voucher`   | 4.7       |
| `execute::relay::execute_receive_acknowledgement` | 4.5  |
| `ibc::receive::pool::ibc_execute_add_liquidity` | 4.0    |

## Reduction Opportunities

### High Impact

1. **Reduce serde monomorphizations in factory/router.** The `euclid::msgs` types are very large enums. Consider:
   - Flattening nested message types where possible
   - Using `#[serde(untagged)]` sparingly on variants that share structure
   - Splitting large `ExecuteMsg` enums into sub-enums dispatched by a wrapper
   - Using raw JSON (`cosmwasm_std::Binary`) for pass-through messages that are just forwarded

2. **Refactor `factory::ibc::ack_and_timeout::reusable_internal_ack_call` (30 KB).** This single function is 3% of the factory binary. It likely has a large match block that could be broken into smaller functions or use a dispatch table.

3. **Unify duplicate dependency versions.** Upgrading the `astroport` dependency (or forking it) to use cosmwasm-std v2 would eliminate the v1/v2 duplicates of cosmwasm-std, cw-storage-plus, cw2, cw20, and others.

### Medium Impact

4. **Audit `.rodata` for verbose error messages.** 62 KB of string data in factory is significant. Consider shorter error codes where the full description can live in documentation instead.

5. **Strip `rustc_demangle` (~6 KB per contract).** If panic backtraces are not useful in production wasm, this can potentially be eliminated with `panic = "abort"` (already set) plus `#[cfg]` gating debug formatters.

### Low Impact

6. **The `to_lowercase` function (4 KB)** appears in every contract. If it's only used for case-insensitive denom comparisons, a simpler ASCII-only lowercase could replace it.

7. **The `base64` crate (7 KB)** appears in every contract via cosmwasm-std. This is unavoidable.
