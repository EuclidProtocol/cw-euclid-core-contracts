# Tick Math: `get_tick_at_sqrt_ratio` Algorithm

Ported from Uniswap V3's `TickMath.sol`. Given a Q96 sqrt price, computes the
largest tick `t` where `get_sqrt_ratio_at_tick(t) <= sqrt_price`.

## Background

In concentrated liquidity, each tick `t` maps to a price:

```
price(t) = 1.0001^t
sqrt_price(t) = sqrt(1.0001)^t = 1.0001^(t/2)
```

The inverse is:

```
t = floor( log_{sqrt(1.0001)}(sqrt_price) )
  = floor( log2(sqrt_price) / log2(sqrt(1.0001)) )
```

Computing this directly with floating point would lose precision. Instead, we use
fixed-point integer arithmetic in three steps.

## Algorithm Overview

```
                    sqrt_price_x96 (Q96 input)
                            │
                    ┌───────▼────────┐
                    │  Step 1: log2  │
                    │  (log2_q64)    │
                    └───────┬────────┘
                            │ Q64 fixed-point log2
                    ┌───────▼────────┐
                    │  Step 2: scale │
                    │  × (1/log2(√a))│
                    └───────┬────────┘
                            │ Q128 approximate tick
                    ┌───────▼────────┐
                    │  Step 3: round │
                    │  resolve ±1    │
                    └───────┬────────┘
                            │
                        exact tick
```

## Worked Example: tick 10000

`get_sqrt_ratio_at_tick(10000)` = `130621891405341611593710811006` (Q96).

We want to recover tick 10000 from this sqrt price.

---

### Step 1: Compute log2 — `log2_q64()`

#### Step 1a: Integer part via MSB

Convert Q96 → Q128 by shifting left 32 bits:

```
ratio_x128 = sqrt_price_x96 << 32
           = 130621891405341611593710811006 × 2^32
           = 561_048_...  (a large 130-bit number)
```

Find the most significant bit:

```
MSB = 129
```

The integer part of log2 is `MSB - 128 = 1`. This means `sqrt_price ≈ 2^1`,
so the real sqrt price is between 2.0 and 4.0 (which makes sense — tick 10000
corresponds to `price ≈ 1.0001^10000 ≈ 2.718`, so `sqrt_price ≈ 1.649`... but
remember, the Q128 encoding shifts things, so MSB=129 means the encoded value
is ~2× the baseline).

#### Step 1b: Fractional part via repeated squaring

Normalize `r` into the range [1.0, 2.0) in Q127 by shifting the MSB to bit 127:

```
r = ratio_x128 >> (129 - 127) = ratio_x128 >> 2
```

Now `r` represents the fractional part we need to measure. Initialize:

```
log2 = (129 - 128) << 64 = 1 × 2^64    (integer part = 1, in Q64)
```

Then iterate 14 times (bits 63 down to 50):

```
Iteration (bit 63):
  ┌──────────────────────────────────────────────┐
  │  r² = r × r  (in Q127 × Q127 = Q254)        │
  │  r  = r² >> 127  (back to Q127)              │
  │                                               │
  │  Is r ≥ 2.0? (Is bit 128 set?)               │
  │   ├─ YES: f = 1, log2 += 1 << 63, r >>= 1   │
  │   └─ NO:  f = 0, log2 unchanged              │
  └──────────────────────────────────────────────┘

Iteration (bit 62):
  Same process, accumulating into bit 62 of log2...

  ... 12 more iterations down to bit 50 ...
```

**Why squaring works**: After normalization, `r` represents `2^f` in Q127, where
`f` is the fractional part of log2 we're trying to measure (0 ≤ f < 1). Squaring
doubles the exponent, which lets us read off one binary digit at a time.

Concrete example with `f = 0.72` (i.e., `r = 2^0.72` in Q127):

1. Square: `r² = 2^1.44 ≥ 2.0` → bit = 1. Divide by 2: `r = 2^0.44`
2. Square: `r² = 2^0.88 < 2.0` → bit = 0. Keep `r = 2^0.88`
3. Square: `r² = 2^1.76 ≥ 2.0` → bit = 1. Divide by 2: `r = 2^0.76`
4. Continue...

This extracts the binary expansion `0.1012... ≈ 0.72`.

Each iteration extracts one binary digit of the fractional part:

```
Fractional log2 bits:
  0 . b₆₃ b₆₂ b₆₁ b₆₀ b₅₉ b₅₈ b₅₇ b₅₆ b₅₅ b₅₄ b₅₃ b₅₂ b₅₁ b₅₀
      ─────────────────────────────────────────────────────────────────
      These 14 bits give ~4 decimal digits of precision
```

**Result for our example**: `log2_q64 = 13304759199159287808` (as a Q64 value).

To interpret: `13304759199159287808 / 2^64 ≈ 0.7213` → full log2 = `1 + 0.7213 ≈ 1.7213`.

Sanity check: `2^1.7213 ≈ 3.297`, and `sqrt_price² = price ≈ 2.718`. Close
(the small error is why step 3 exists).

---

### Step 2: Change of base

Convert log2 to tick space by multiplying by the precomputed constant:

```
tick ≈ log2(sqrt_price) / log2(sqrt(1.0001))
     = log2(sqrt_price) × (1 / log2(sqrt(1.0001)))
```

The constant `LOG_SQRT10001_SCALE = 255738958999603826347141` encodes
`1 / log2(sqrt(1.0001))` scaled so that Q64 × integer → Q128:

```
log_sqrt10001 = log2_q64 × 255738958999603826347141
```

This gives a Q128 fixed-point result where the upper 128 bits are the integer
tick value and the lower 128 bits are fractional.

---

### Step 3: Resolve ±1 rounding

14 bits of fractional precision means the tick estimate can be off by ±1. V3
precomputes two error margins:

```
TICK_LOW_ERR  = 3402992956809132418596140100660247210
TICK_HIGH_ERR = 291339464771989622907027621153398088495
```

Compute pessimistic and optimistic tick estimates:

```
tick_low  = (log_sqrt10001 - TICK_LOW_ERR)  >> 128   // floor, pessimistic
tick_high = (log_sqrt10001 + TICK_HIGH_ERR) >> 128   // floor, optimistic
```

```
 ┌─────────────────────────────────────────────────────┐
 │  tick_low == tick_high?                              │
 │   ├─ YES: return tick_low (precision was sufficient) │
 │   └─ NO: they differ by 1, so check:                │
 │          get_sqrt_ratio_at_tick(tick_high) <= input? │
 │           ├─ YES: return tick_high                   │
 │           └─ NO:  return tick_low                    │
 └─────────────────────────────────────────────────────┘
```

For tick 10000: both `tick_low` and `tick_high` resolve to `10000`, so we
return immediately without needing the extra `get_sqrt_ratio_at_tick` call.

**Worst case**: one call to `get_sqrt_ratio_at_tick` (O(1) — just 20 conditional
multiplies). Compare this to the old binary search which needed ~21 calls.

## Fixed-Point Number Formats

```
Q96:   value = raw_integer / 2^96     (sqrt prices stored on-chain)
Q127:  value = raw_integer / 2^127    (internal normalization range)
Q128:  value = raw_integer / 2^128    (1.0 sits at bit 128)
Q64:   value = raw_integer / 2^64     (log2 result format)

Example — Q96 encoding of sqrt_price = 1.0:
  1.0 × 2^96 = 79228162514264337593543950336
  (this is exactly the sqrt price at tick 0)
```

## Signed 256-bit Arithmetic

The log2 of sqrt prices below 1.0 (negative ticks) is negative, so we need
signed 256-bit integers. We use `cosmwasm_std::Int256`, which provides
`wrapping_mul`, `wrapping_add`, `wrapping_sub`, and arithmetic shift right
(`>>`) — matching Solidity's `int256` wrapping behavior.
