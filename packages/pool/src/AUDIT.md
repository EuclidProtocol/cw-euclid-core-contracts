# Stable Swap Math: Security Audit Report

**Date:** 2026-03-25
**Scope:** `packages/pool/src/stable_math.rs` and its integration via `pool_functions.rs`
**Context:** Voucher balances are 24-decimal fixed-point values stored as `Uint128`. The stable swap math operates on `Decimal256` (18 internal decimal places). This report identifies vulnerabilities in the current implementation.

## Summary

| ID | Severity | Title | Status |
|----|----------|-------|--------|
| CRITICAL-1 | Critical | Overflow in `d^3` computation makes math unusable for 24-decimal tokens | Open |
| CRITICAL-2 | Critical | Unchecked arithmetic in `compute_d` causes contract panic | Open |
| CRITICAL-3 | Critical | `TOKEN_PRECISION = 1` is hardcoded and unrelated to token decimals | Open |
| HIGH-1 | High | Inconsistent amp factor scaling between `compute_d` and `calc_y` | Open |
| HIGH-2 | High | `saturating_sub` silently masks bugs in spread calculation | Open |
| MEDIUM-1 | Medium | Fixed absolute tolerance is not scale-aware | Open |
| MEDIUM-2 | Medium | No input validation on amp factor or pool amounts | Open |
| LOW-1 | Low | Integer truncation in return_amount creates systematic user loss | Open |

## Findings

### CRITICAL-1: Overflow in `d.pow(3)` / `d.checked_pow(3)`

**Location:** `stable_math.rs:93` (unchecked `d.pow(3)`), `stable_math.rs:125` (checked `d.checked_pow(3)`)

**Description:**
Both `compute_d` and `calc_y` compute `D^3` as part of the StableSwap formula. `Decimal256` stores values as `atomics = value * 10^18` in a `Uint256` (max ~ `1.158 * 10^77`). For any pool with integer values >= ~1e20, the intermediate `D^3` computation overflows `Uint256`.

With 24-decimal tokens (where 1 token = `10^24` raw), even a single token per pool overflows.

**Overflow boundary analysis:**

| Pool Integer Value | d atomics | d^3 atomics | Overflows? | Behavior |
|--------------------|-----------|-------------|------------|----------|
| 1e3 (small) | 2e21 | 8e27 | No | Works |
| 1e12 | 2e30 | 8e54 | No | Works |
| 1e18 (current test max) | 2e36 | 8e72 | No | Works (ratio to max: 6.9e-5) |
| **1e19** | **2e37** | **8e75** | **Yes** | Returns `Err` (overflow in `calc_y`'s `d^3 * amp_prec` step, since 8e75 * 100 > Uint256 max) |
| **1e20** | **2e38** | **8e78** | **Yes** | **Panics** (overflow in `compute_d`'s unchecked `d.pow(3)`) |
| **1e24 (1 token at 24 dec)** | **2e42** | **8e90** | **Yes** | **Panics** (even intermediate d*d overflows) |

Note: At 1e19, `d.pow(3)` itself fits in Uint256, but `calc_y` then multiplies by `amp_prec = 100`, causing the overflow in a checked path that returns a clean `Err`. At 1e20+, `d.pow(3)` itself overflows and hits the unchecked code path in `compute_d`, causing a contract panic.

Even the intermediate `d * d` overflows for 24-decimal values: `(2e42)^2 = 4e84 >> 1.158e77`.

**Impact:** Complete inability to perform stable swaps with 24-decimal voucher tokens. In `compute_d`, the unchecked `d.pow(3)` will panic, halting the contract. In `calc_y`, the checked version returns an error, but the swap still fails.

**Failing test:** `test_overflow_24_decimal_balanced_pools`, `test_overflow_24_decimal_small_pools`

**Suggested fix:**
Replace `d.pow(3) / (pool_a_scaled * pool_b_scaled)` with iterative computation that avoids the large intermediate:

```rust
// Instead of: d.pow(3) / (amount_a_times_coins * amount_b_times_coins)
// Use iterative: d_product = d * d / pool_a_scaled * d / pool_b_scaled
let mut d_product = d;
for pool in [amount_a_times_coins, amount_b_times_coins] {
    d_product = d_product.checked_mul(d)?.checked_div(pool)?;
}
```

Similarly in `calc_y`, restructure `d.checked_pow(3)?.checked_mul(amp_prec)?` to avoid the `D^3` intermediate.

---

### CRITICAL-2: Unchecked arithmetic in `compute_d` causes contract panic

**Location:** `stable_math.rs:80-93`

**Description:**
Several operations in `compute_d` use unchecked arithmetic that will panic on overflow instead of returning a recoverable error:

```rust
// Line 80: unchecked multiply
let leverage = Decimal256::from_ratio(amp, AMP_PRECISION) * N_COINS;

// Line 81-82: unchecked multiply
let amount_a_times_coins = pools[0] * N_COINS;
let amount_b_times_coins = pools[1] * N_COINS;

// Line 93: unchecked pow, unchecked divide, unchecked multiply
let d_product = d.pow(3) / (amount_a_times_coins * amount_b_times_coins);
```

**Impact:** A contract panic in CosmWasm is unrecoverable for that transaction. While it doesn't corrupt state (the transaction rolls back), it means users cannot perform swaps and receive an unhelpful generic error. Funds are not directly at risk but pool functionality is denied.

**Failing test:** `test_compute_d_unchecked_panics`

**Suggested fix:**
Replace all unchecked operations with checked variants:

```rust
let leverage = Decimal256::from_ratio(amp, AMP_PRECISION).checked_mul(N_COINS)?;
let amount_a_times_coins = pools[0].checked_mul(N_COINS)?;
let amount_b_times_coins = pools[1].checked_mul(N_COINS)?;

// In loop:
let d_product = d.checked_pow(3)?
    .checked_div(amount_a_times_coins.checked_mul(amount_b_times_coins)?)?;
```

---

### CRITICAL-3: `TOKEN_PRECISION = 1` is hardcoded and unrelated to token decimals

**Location:** `stable_math.rs:21`

**Description:**
`TOKEN_PRECISION` is a local constant set to `1` inside `compute_stable_swap`. It is used for:

1. `ask_pool.to_uint128_with_precision(1)` — divides atomics by `10^17`, giving `value * 10`
2. `calc_y(..., TOKEN_PRECISION)` — returns `y` with the same `10x` scaling
3. `checked_div(Uint128::new(10))` — removes the `10x` scaling from the subtraction result
4. `offer_asset.to_uint128_with_precision(0)` — gives the raw integer value

The purpose is to preserve one extra decimal digit during the `ask_pool - new_ask_pool` subtraction to reduce rounding error. However:

- This constant has no relationship to the actual decimal configuration of the tokens (6, 18, 24, etc.)
- For 24-decimal tokens, the return amount is in raw 24-decimal units, and the extra digit of precision from TOKEN_PRECISION=1 is negligible relative to 24 decimal places
- The spread calculation `offer_amount.saturating_sub(return_amount)` compares two values that were converted with different precisions (0 vs 1-then-divided-by-10), which are equivalent only because the math works out for integer inputs

**Impact:** While the current implementation produces correct results for integer-valued inputs (the precision(1) and /10 cancel out), it does not account for fractional Decimal256 values or non-integer token amounts. Any refactoring that changes how amounts enter the function could silently break the precision math.

**Failing test:** `test_precision_with_24_decimal_inputs`

**Suggested fix:**
Either:
1. Accept token decimal configuration as a parameter and normalize amounts before computation
2. Perform all math in Decimal256 and only convert to Uint128 at the final step
3. At minimum, document why TOKEN_PRECISION=1 is correct and add assertions that inputs are integer-valued

---

### HIGH-1: Inconsistent amp factor scaling between `compute_d` and `calc_y`

**Location:** `stable_math.rs:80` vs `stable_math.rs:122-123`

**Description:**
The two functions parameterize the amplification factor differently:

```rust
// compute_d (line 80):
let leverage = Decimal256::from_ratio(amp, AMP_PRECISION) * N_COINS;
// leverage = (amp / 100) * 2

// calc_y (lines 122-123):
let leverage = Decimal256::from_ratio(amp, 1u8) * N_COINS;
let amp_prec = Decimal256::from_ratio(AMP_PRECISION, 1u8);
// leverage = amp * 2, amp_prec = 100 (used separately in formulas)
```

These are mathematically equivalent: `calc_y` factors out `AMP_PRECISION` and applies it separately in the `c` and `b` formulas. However, the inconsistency is dangerous:

- A developer modifying `compute_d` to change how `leverage` is computed would need to know that `calc_y` uses a different factoring
- The relationship is not documented
- Code review is harder because the two functions appear to use different amplification values

**Verification:** The following identity holds:
- `compute_d`: `Ann = (amp/100) * 2`
- `calc_y`: `c = D^3 * 100 / (x * 4 * amp * 2) = D^3 / (x * 4 * (amp/100) * 2) = D^3 / (x * N_COINS^2 * Ann)` (correct)

**Failing test:** `test_amp_factor_consistency`

**Suggested fix:**
Unify both functions to use the same leverage computation. The simplest approach is to have `calc_y` also use `Decimal256::from_ratio(amp, AMP_PRECISION) * N_COINS`.

---

### HIGH-2: `saturating_sub` silently masks bugs in spread calculation

**Location:** `stable_math.rs:41`

**Description:**
```rust
let spread_amount = offer_amount.saturating_sub(return_amount);
```

If `return_amount > offer_amount`, this silently returns `0` instead of signaling an invariant violation. In a correctly functioning StableSwap, the return amount should always be less than or equal to the offer amount (the pool takes a spread). If it's not, something is mathematically wrong.

Using `saturating_sub` here means:
- Arbitrage conditions where users get more than they put in would be hidden
- Math bugs that produce inflated return amounts would go undetected
- The spread amount reported to users would be incorrect (0 instead of an error)

**Impact:** Could mask fund-draining bugs where the pool pays out more than it receives.

**Failing test:** `test_spread_can_silently_be_zero`

**Suggested fix:**
```rust
let spread_amount = offer_amount
    .checked_sub(return_amount)
    .map_err(|_| ContractError::new(
        "Invariant violation: return_amount exceeds offer_amount"
    ))?;
```

---

### MEDIUM-1: Fixed absolute tolerance is not scale-aware

**Location:** `stable_math.rs:12`

**Description:**
```rust
pub const TOL: Decimal256 = Decimal256::raw(1000000000000); // 1e-6
```

This absolute tolerance of `1e-6` is used for convergence checks in both `compute_d` and `calc_y`:
```rust
if d.abs_diff(d_previous) <= TOL { return Ok(d); }
```

For large values (e.g., 24-decimal pools where D ~ 1e24), a tolerance of 1e-6 is `1e-30` relative to the value, meaning convergence happens very quickly but the last few iterations may be wasted or the result may lack precision in the least significant digits.

For very small values (e.g., pools with 1 unit), `1e-6` relative to `1` means we stop iterating when within `0.0001%`, which may be acceptable but is not consistent with the large-value behavior.

**Impact:** Potential precision loss for edge cases; not a correctness issue for typical values.

**Note:** No dedicated test for this finding. The tolerance concern is theoretical for current usage (integer values << 1e18) but would become relevant if the overflow issues (CRITICAL-1) are fixed to support larger values.

**Suggested fix:**
Consider a relative tolerance:
```rust
if d.abs_diff(d_previous) <= TOL || d.abs_diff(d_previous) / d <= relative_tol {
    return Ok(d);
}
```

---

### MEDIUM-2: No input validation on amp factor or pool amounts

**Location:** `compute_stable_swap`, `compute_d`, `calc_y`

**Description:**
None of the math functions validate their inputs:

- **Zero amp factor**: `amp = 0` causes `leverage = 0`. In `calculate_step`, the expression `leverage - Decimal256::one()` underflows (unsigned subtraction of 0 - 1), causing a **panic** instead of a clean error.
- **Extremely large amp factor**: Could overflow the `leverage` multiplication
- **Zero pool amounts**: `pools[0] = 0` with non-zero `pools[1]` causes division by zero in `d_product = d^3 / (amount_a * amount_b)` on line 93
- **Pool array length**: `compute_d` directly indexes `pools[0]` and `pools[1]` without checking the array length (though this is somewhat mitigated by it being called only from trusted internal code)

**Impact:** Unvalidated inputs cause panics or incorrect results instead of descriptive errors.

**Failing test:** `test_zero_amp_factor_panics`, `test_extreme_amp_factor`, `test_zero_pool_reserve_panics`

**Suggested fix:**
Add input validation at the entry point (`compute_stable_swap`):
```rust
ensure!(amp_factor.u64() > 0, ContractError::new("Amp factor must be positive"));
ensure!(amp_factor.u64() <= MAX_AMP, ContractError::new("Amp factor exceeds maximum"));
ensure!(!offer_pool.is_zero() && !ask_pool.is_zero(), ContractError::new("Pool reserves must be non-zero"));
```

---

### LOW-1: Integer truncation in return_amount creates systematic user loss

**Location:** `stable_math.rs:35`

**Description:**
```rust
let return_amount = ask_pool_amount
    .checked_sub(new_ask_pool_amount)?
    .checked_div(Uint128::new(10u128.pow(TOKEN_PRECISION as u32)))?;
```

The `checked_div(10)` performs floor division, always rounding down. This means:
- A difference of `15` becomes `1` (user loses 5 units of the extra-precision digit)
- A difference of `9` becomes `0` (user gets nothing for a valid but small swap)
- Across many swaps, this creates a systematic leak of value away from users and into the pool

For very small swaps (1-2 units in integer terms), the rounding error can be 50% or more of the swap value.

**Impact:** Small but systematic value extraction from users. Accumulates over time and across all swaps.

**Failing test:** `test_truncation_loss_small_swaps`, `test_truncation_loss_imbalanced_pools`

**Suggested fix:**
Use rounding division instead of floor division:
```rust
let divisor = Uint128::new(10u128.pow(TOKEN_PRECISION as u32));
let return_amount = (diff + divisor / Uint128::new(2)) / divisor; // round to nearest
```

Or eliminate TOKEN_PRECISION entirely and perform the subtraction in Decimal256 before converting to Uint128 once.

---

## Recommendations

### Immediate (before any production deployment with 24-decimal tokens)

1. **Fix CRITICAL-1**: Replace `d.pow(3)` with iterative `d_product` computation to avoid Uint256 overflow. This is the single most important fix as it completely blocks functionality.

2. **Fix CRITICAL-2**: Replace all unchecked arithmetic in `compute_d` with checked variants to prevent contract panics.

3. **Fix CRITICAL-3**: Either normalize 24-decimal amounts down to a safe range before entering the math, or restructure the math to work with arbitrary decimal configurations.

### Short-term

4. **Fix HIGH-2**: Replace `saturating_sub` with `checked_sub` + error in the spread calculation to catch invariant violations.

5. **Fix HIGH-1**: Unify amp factor parameterization between `compute_d` and `calc_y`.

6. **Fix MEDIUM-2**: Add input validation for amp factor bounds and pool amounts.

### Long-term

7. Consider a comprehensive rewrite referencing the Curve/Astroport implementations that handle these edge cases.

8. Add property-based fuzz testing (the `tests-fuzz` framework already exists in this repo) to verify invariants like: `return_amount <= offer_amount`, `D_after >= D_before` for deposits, and `no overflow for any valid input range`.

9. Consider using `Uint512` or `checked_multiply_ratio` approaches to avoid intermediate overflow in the `D^3` computation.
