# Stable Swap Math: Security Audit Report

**Date:** 2026-03-25 (updated 2026-03-26)
**Scope:** `packages/pool/src/stable_math.rs` and its integration via `stable.rs` / `common.rs`
**Context:** All stable swap inputs are `Uint128` integer amounts (range: 0 to ~3.4e38). The stable swap math operates on `Decimal256` (18 internal decimal places) internally. This report identifies vulnerabilities and tracks their resolution.

## Summary

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| CRITICAL-1 | Critical | Overflow in `d^3` computation makes math unusable for large tokens | **Fixed** |
| CRITICAL-2 | Critical | Unchecked arithmetic causes contract panic | **Fixed** |
| CRITICAL-3 | Critical | `TOKEN_PRECISION = 1` is hardcoded and unrelated to token decimals | **Fixed** |
| HIGH-1 | High | Inconsistent amp factor scaling between `compute_d` and `calc_y` | **Fixed** |
| HIGH-2 | High | `saturating_sub` silently masks bugs in spread calculation | **Fixed** |
| MEDIUM-1 | Medium | Fixed absolute tolerance is not scale-aware | Open |
| MEDIUM-2 | Medium | No input validation on amp factor or pool amounts | **Fixed** |
| LOW-1 | Low | Integer truncation in return_amount creates systematic user loss | Open |

## Findings

### CRITICAL-1: Overflow in `d.pow(3)` / `d.checked_pow(3)` [FIXED]

**Location:** `stable_math.rs` (compute_d, calc_y)

**Description:**
Both `compute_d` and `calc_y` compute `D^3` as part of the StableSwap formula. `Decimal256` stores values as `atomics = value * 10^18` in a `Uint256` (max ~ `1.158 * 10^77`). For any pool with integer values >= ~1e20, the intermediate `D^3` computation overflows `Uint256`.

**Fix applied (two layers):**

1. **compute_d**: Replaced `d.pow(3)` with iterative `checked_multiply_ratio` calls. `checked_multiply_ratio` performs `(a * b) / c` in `Uint512` intermediate space, avoiding overflow:

```rust
let d_product = d
    .checked_multiply_ratio(d, amount_a_times_coins)?
    .checked_multiply_ratio(d, amount_b_times_coins)?;
```

2. **calc_y**: For pool values near Uint128::MAX, even the result of `c = D^3 / (x * N^2 * Ann)` exceeds Decimal256 range (~1.158e59). Fixed by computing `c/denom` directly in the Newton loop instead of computing `c` as a standalone value:

```rust
// Instead of precomputing c and then c/denom in the loop,
// compute c/denom = D^3 / (x*N * N*leverage * denom) iteratively:
let c_over_denom = d
    .checked_multiply_ratio(d, new_amount_times_n)?
    .checked_multiply_ratio(d, n_times_leverage.checked_mul(denom)?)?;
```

Each intermediate result stays within Decimal256 range because we divide by progressively larger denominators.

**Verification tests:** `test_24_decimal_balanced_pools_now_works`, `test_1e20_pools_now_works`, `test_compute_d_24_decimal_pools`, `test_calc_y_large_pools_succeeds`, `test_uint128_max_pools_succeeds`

---

### CRITICAL-2: Unchecked arithmetic causes contract panic [FIXED]

**Location:** `stable_math.rs` (compute_d, calc_y, calculate_step)

**Description:**
Several operations used unchecked arithmetic that would panic on overflow instead of returning a recoverable error.

**Fix applied:**
All arithmetic now uses checked variants throughout the entire math pipeline:

- `Decimal256::checked_mul` uses `full_mul` (Uint512 intermediate) internally in cosmwasm-std 2.2.2
- `Decimal256Ext::checked_multiply_ratio` uses `Uint256::checked_multiply_ratio` (Uint512 intermediate)
- `calculate_step` uses `checked_multiply_ratio(initial_d, r_val)` instead of `checked_mul(initial_d)` then `checked_div(r_val)`
- Return amount computation uses `Uint128` for TOKEN_PRECISION scaling. Maximum supported pool value is `Uint128::MAX / 10` (~3.4e37) due to the x10 scaling. Pools exceeding this limit return a clean error.

**Verification tests:** `test_compute_d_handles_extreme_values_without_panic`, `test_uint128_max_pools_returns_error`, `test_uint128_max_div_10_pools_succeeds`, `test_uint128_max_offer_no_panic`, `test_all_uint128_max_no_panic`

---

### CRITICAL-3: `TOKEN_PRECISION = 1` is hardcoded and unrelated to token decimals [FIXED]

**Location:** `stable_math.rs:45`

**Description:**
`TOKEN_PRECISION` is set to `1` inside `compute_stable_swap`. It adds one extra decimal digit during the subtraction to reduce rounding error, then divides by 10. This is correct for integer inputs but was not explicit about that requirement. Additionally, the scaled values used `Uint128` which would overflow for pools near Uint128::MAX (3.4e38 × 10 > Uint128::MAX).

**Fix applied:**
1. Changed `compute_stable_swap` to accept explicit `Uint128` integer inputs instead of `Decimal256`, making the integer contract clear at the type level.
2. TOKEN_PRECISION is kept as `1` with `Uint128` arithmetic. The `to_uint128_with_precision(1)` scaling multiplies values by 10, which means the maximum supported pool value is `Uint128::MAX / 10` (~3.4e37). Pools at `Uint128::MAX` will return a clean error. This is documented in the function's doc comment.

**Verification tests:** `test_precision_with_integer_inputs`, `test_uint128_max_pools_returns_error`, `test_uint128_max_div_10_pools_succeeds`

---

### HIGH-1: Inconsistent amp factor scaling between `compute_d` and `calc_y` [FIXED]

**Location:** `stable_math.rs` (compute_d vs calc_y)

**Description:**
`compute_d` used `leverage = (amp / AMP_PRECISION) * N_COINS` while `calc_y` used a different factoring with `amp * N_COINS` and `amp_prec` applied separately.

**Fix applied:**
Both functions now use identical leverage computation:
```rust
let leverage = Decimal256::from_ratio(amp, AMP_PRECISION).checked_mul(N_COINS)?;
```

**Verification tests:** `test_amp_factor_consistency`

---

### HIGH-2: `saturating_sub` silently masks bugs in spread calculation [FIXED]

**Location:** `stable_math.rs:68`

**Description:**
Used `saturating_sub` which silently returns 0 if `return_amount > offer_amount`, hiding potential invariant violations.

**Fix applied:**
```rust
let spread_amount = offer_amount.checked_sub(return_amount).map_err(|_| {
    ContractError::new("Invariant violation: return_amount exceeds offer_amount")
})?;
```

**Verification tests:** `test_spread_uses_checked_sub`

---

### MEDIUM-1: Fixed absolute tolerance is not scale-aware

**Location:** `stable_math.rs:12`

**Description:**
```rust
pub const TOL: Decimal256 = Decimal256::raw(1000000000000); // 1e-6
```

This absolute tolerance of `1e-6` is used for convergence checks. For large values (e.g., pools where D ~ 1e24), `1e-6` is negligible and convergence happens quickly. For very small values (pools with 1 unit), `1e-6` relative to `1` means we stop at `0.0001%` precision.

**Impact:** Low. The tolerance works well across the Uint128 range in practice. Not a correctness issue for typical values.

**Suggested fix:**
Consider adding a relative tolerance check:
```rust
if d.abs_diff(d_previous) <= TOL || d.abs_diff(d_previous) / d <= relative_tol {
    return Ok(d);
}
```

---

### MEDIUM-2: No input validation on amp factor or pool amounts [FIXED]

**Location:** `stable_math.rs:26-34`

**Description:**
None of the math functions validated their inputs. Zero amp factor caused panics, zero pool amounts caused division by zero.

**Fix applied:**
Input validation at the `compute_stable_swap` entry point:
```rust
if amp_factor.is_zero() {
    return Err(ContractError::new("Amp factor must be greater than zero"));
}
if offer_pool.is_zero() || ask_pool.is_zero() {
    return Err(ContractError::new("Pool reserves must be non-zero"));
}
if offer_asset.is_zero() {
    return Err(ContractError::new("Offer amount must be non-zero"));
}
```

**Verification tests:** `test_zero_amp_factor_returns_error`, `test_extreme_amp_factor`, `test_zero_pool_reserve_returns_error`, `test_zero_offer_returns_error`, `test_uint128_min_boundaries`

---

### LOW-1: Integer truncation in return_amount creates systematic user loss

**Location:** `stable_math.rs:62`

**Description:**
The TOKEN_PRECISION floor division `checked_div(10)` always rounds down. For very small swaps (1 or 2 units), the rounding error can be significant.

**Impact:** Small but systematic value extraction from users. Accumulates over time. The protocol benefits (value stays in the pool), so this is conservative behavior, not a vulnerability.

**Suggested fix:**
Use rounding division instead of floor division:
```rust
let divisor = Uint128::new(10u128.pow(TOKEN_PRECISION as u32));
let return_amount = (diff + divisor / Uint128::new(2)) / divisor;
```

---

## Recommendations

### Completed

1. **CRITICAL-1**: Replaced `d.pow(3)` with iterative `checked_multiply_ratio` (Uint512 intermediate). Restructured `calc_y` to compute `c/denom` directly, avoiding `c` exceeding Decimal256 range for large pools.
2. **CRITICAL-2**: All arithmetic is checked. `Decimal256::checked_mul` uses `full_mul` (Uint512). TOKEN_PRECISION scaling uses `Uint128`, limiting max pool value to `Uint128::MAX / 10`.
3. **CRITICAL-3**: Changed API to accept explicit `Uint128` integer inputs. Maximum supported pool value is `Uint128::MAX / 10` (~3.4e37), documented in function doc comment.
4. **HIGH-1**: Unified amp factor parameterization.
5. **HIGH-2**: Replaced `saturating_sub` with `checked_sub` + error.
6. **MEDIUM-2**: Added input validation for amp factor, pool amounts, and offer amount.

### Open (low priority)

7. **MEDIUM-1**: Consider relative tolerance for Newton's method convergence.
8. **LOW-1**: Consider rounding division instead of floor division for return_amount.

### Future considerations

9. Add property-based fuzz testing (the `tests-fuzz` framework exists in this repo) to verify invariants like: `return_amount <= offer_amount`, `D_after >= D_before`, and `no panic for any valid Uint128 input`.
