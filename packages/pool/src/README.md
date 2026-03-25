# Stable Swap Math

This module implements a Curve-style StableSwap automated market maker (AMM) for 2-coin pools. StableSwap provides low-slippage swaps for assets that are expected to trade near parity (e.g., stablecoin pairs, wrapped/unwrapped pairs).

## Mathematical Foundation

The StableSwap invariant combines constant-sum (zero slippage) and constant-product (infinite liquidity) behaviors, controlled by an amplification parameter `A`:

```
A * n^n * sum(x_i) + D = A * n^n * D + D^(n+1) / (n^n * prod(x_i))
```

Where:
- `n` = number of coins (always 2 in this implementation)
- `x_i` = pool balance of coin `i`
- `D` = the invariant (total value of the pool when balanced)
- `A` = amplification coefficient (higher = closer to constant-sum behavior)

## Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `N_COINS` | `2.0` (Decimal256) | Number of coins in the pool |
| `AMP_PRECISION` | `100` | Divisor for the raw amplification parameter. Effective A = amp / 100. |
| `ITERATIONS` | `64` | Maximum Newton's method iterations before returning an error |
| `TOL` | `1e-6` (Decimal256) | Absolute convergence tolerance for Newton's method |
| `TOKEN_PRECISION` | `1` (local to `compute_stable_swap`) | Extra decimal digit used during return amount subtraction to reduce rounding error |

## Functions

### `compute_stable_swap(offer_asset, offer_pool, ask_pool, amp_factor) -> SwapResult`

**Entry point.** Computes how much the user receives for a given offer amount.

**Flow:**
1. Constructs pool array `xp = [offer_pool, ask_pool]`
2. Calls `calc_y(amp_factor, offer_pool + offer_asset, xp, TOKEN_PRECISION)` to find the new ask pool balance
3. Computes `return_amount = (ask_pool_scaled - new_ask_pool) / 10` (the `/10` reverses the TOKEN_PRECISION=1 scaling)
4. Computes `spread_amount = offer_amount - return_amount` (the slippage/price impact)
5. Returns `SwapResult { return_amount, spread_amount }`

**Parameters:**
- `offer_asset: &Decimal256` — amount the user is swapping in (post-fee)
- `offer_pool: &Decimal256` — current reserve of the offer token
- `ask_pool: &Decimal256` — current reserve of the ask token
- `amp_factor: Uint64` — raw amplification parameter (divided by AMP_PRECISION internally)

### `compute_d(amp, pools) -> Decimal256`

Computes the StableSwap invariant `D` using Newton's method.

**Algorithm:**
1. Computes `leverage = (amp / AMP_PRECISION) * N_COINS`
2. Initializes `d = sum(pools)` (sum of all pool balances)
3. Iterates Newton's method via `calculate_step`:
   ```
   d_product = d^3 / (pool_a * N_COINS * pool_b * N_COINS)
   d_new = (leverage * sum_x + d_product * n_coins) * d / ((leverage - 1) * d + (n_coins + 1) * d_product)
   ```
4. Converges when `|d_new - d_prev| <= TOL`

### `calc_y(amp, new_amount, xp, target_precision) -> Uint128`

Solves for the new balance of the ask token after a swap, given the new offer token balance.

**Algorithm:**
1. Computes `D` via `compute_d`
2. Sets up the quadratic: `y^2 + b*y = c`
   - `c = D^3 * AMP_PRECISION / (new_amount * N_COINS^2 * leverage)` where `leverage = amp * N_COINS`
   - `b = new_amount + D * AMP_PRECISION / leverage`
3. Iterates: `y_new = (y^2 + c) / (2y + b - D)`
4. Returns `y` converted to `Uint128` with target precision

**Note on amp factor parameterization:** `calc_y` uses `leverage = amp * N_COINS` (without dividing by AMP_PRECISION), then compensates by multiplying `c` by `amp_prec = 100`. This is mathematically equivalent to `compute_d`'s approach but uses a different factoring.

### `calculate_step(initial_d, leverage, sum_x, d_product) -> Decimal256`

Newton's method helper for `compute_d`. Computes one iteration step:

```
d_new = (leverage * sum_x + d_product * n_coins) * initial_d
        / ((leverage - 1) * initial_d + (n_coins + 1) * d_product)
```

## Precision Handling

### Decimal256 Internals
- `Decimal256` has 18 internal decimal places, backed by `Uint256`
- `atomics = integer_value * 10^18`
- `Uint256` max ~ `1.158 * 10^77`

### Decimal256Ext Methods (from `euclid::utils::math`)
- `from_integer(x)`: Creates `Decimal256` with integer value `x` (atomics = x * 10^18)
- `to_uint128_with_precision(p)`: Returns `atomics / 10^(18 - p)`
- `with_precision(value, p)`: Creates `Decimal256` from atomics at given precision

### How Precision Is Used in compute_stable_swap
1. `ask_pool.to_uint128_with_precision(1)` gives `value * 10` (one extra digit)
2. `calc_y` returns with the same precision(1) scaling
3. The subtraction `ask_pool_scaled - new_ask_pool` preserves that extra digit
4. Division by `10^TOKEN_PRECISION = 10` removes the extra digit, truncating
5. `offer_asset.to_uint128_with_precision(0)` gives the raw integer value

## Call Flow in the Swap Pipeline

```
User submits swap(amount_in, asset_in)
    |
    v
pre_swap (pool_functions.rs:528)
    |-- Calculate fees: lp_fee + euclid_fee
    |-- swap_amount = amount_in - fees
    |-- Convert to Decimal256: Decimal256::from_integer(swap_amount)
    |-- Convert reserves: Decimal256::from_integer(token_in_reserve)
    |
    v
compute_stable_swap (stable_math.rs:14)
    |-- calc_y -> compute_d -> calculate_step (Newton's method)
    |-- return SwapResult { return_amount, spread_amount }
    |
    v
execute_swap (pool_functions.rs:589)
    |-- Update reserves: in += swap_amount + lp_fee, out -= receive_amount
    |-- Transfer euclid_fee to fee recipient
    |-- Transfer receive_amount to user (or chain to next swap)
```

## Known Limitations

See [AUDIT.md](./AUDIT.md) for a comprehensive security audit including critical overflow issues with large token decimal configurations and precision handling concerns.
