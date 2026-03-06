# Tick Bitmap

Efficient next-initialized-tick lookup during swaps.

## Overview

Every initialized tick (one with non-zero liquidity) is recorded as a single bit in a compact bitmap stored in `TICK_BITMAP: Map<i64, Uint256>`. Each storage entry is a 256-bit word covering 256 consecutive compressed ticks. The swap loop queries this bitmap to find the next tick boundary instead of scanning the full `TICKS` map.

## Tick to Bitmap Mapping

```
tick ──→ compressed = floor(tick / tick_spacing)
         compressed ──→ word_pos = floor(compressed / 256)
                        bit_pos  = compressed mod 256
```

For example, with `tick_spacing = 10`:
- Tick 600  → compressed 60  → word 0, bit 60
- Tick 2560 → compressed 256 → word 1, bit 0
- Tick -100 → compressed -10 → word -1, bit 246

The inverse (`tick_from_word_and_bit`) reconstructs the tick: `tick = (word_pos * 256 + bit_pos) * tick_spacing`.

## Search Algorithm

`find_next_initialized_tick` (in `contract.rs`) uses the bitmap as follows:

1. Map `current_tick` to `(word_pos, bit_pos)`.
2. Load `TICK_BITMAP[word_pos]` — one storage read covers 256 ticks.
3. Scan the word for the nearest set bit in the swap direction:
   - **Descending** (zero_for_one): scan from `bit_pos` down to 0.
   - **Ascending** (!zero_for_one): scan from `bit_pos + 1` up to 255, excluding the current tick.
4. If no set bit is found in the current word, iterate adjacent words via `TICK_BITMAP.range()` in the same direction until one is found.
5. If no initialized tick exists, return `MIN_TICK` or `MAX_TICK`.

The within-word scan (`next_initialized_bit_in_word`) operates on the LE byte representation — at most 32 byte checks using `leading_zeros` / `trailing_zeros` — rather than 256 individual `Uint256` operations.

## Why This Is Faster

The previous approach used `TICKS.range().next()`, a B-tree seek across every initialized tick's full `TickInfo` struct. The bitmap approach:
- Reads a single compact `Uint256` per word (vs. a `TickInfo` with liquidity, fee growth fields, etc.)
- Covers 256 ticks per storage read (common case: one read is enough)
- Empty words are not stored, so the range fallback skips gaps efficiently

## Storage Maintenance

The bitmap is maintained in `update_tick` (contract.rs): when a tick transitions between initialized/uninitialized, `set_bit` or `clear_bit` updates the corresponding word. Words that become all-zero are removed from storage.

## Bitwise Helpers

`Uint256` in cosmwasm-std 2.2.2 implements `Not` and `Shl`/`Shr` but not `BitOr` or `BitAnd`. The module provides byte-level `bitor`/`bitand` helpers that convert to `[u8; 32]` LE, apply native byte ops, and convert back. `set_bit`, `clear_bit`, and `is_set` are built on these and are idempotent by construction (no guards needed).
