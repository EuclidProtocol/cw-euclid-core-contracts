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

## Why Not `TICKS.range()`?

The previous approach used `TICKS.range().next()` to find the next initialized tick. The bitmap replaces this with a direct key lookup + in-memory scan. The `TickInfo` read still happens when the swap *crosses* a tick — the bitmap only optimizes *finding* it.

For a typical swap crossing 1-2 nearby ticks, the gas difference is small — one direct load vs one iterator seek are both single storage operations. The bitmap's real advantages show up at the edges:

- **Sparse pools:** When the next initialized tick is far away, `TICKS.range()` must seek across the full keyspace. The bitmap skips empty 256-tick regions in one step since empty words aren't stored.
- **Dense multi-step swaps:** If consecutive ticks fall within the same 256-tick word, the second lookup is a pure in-memory scan with no additional storage access.
- **Predictable gas:** Cost stays bounded regardless of how many ticks exist in the pool, whereas iterator performance depends on keyspace size.

The bitmap adds a small write cost (`set_bit`/`clear_bit` on tick init/uninit) and storage overhead (~1 `Uint256` per 256 active ticks). Both are negligible — the write happens on a path that already touches storage, and a pool with 1000 ticks needs ~4 extra words (128 bytes).

## Storage Maintenance

The bitmap is maintained in `update_tick` (contract.rs): when a tick transitions between initialized/uninitialized, `set_bit` or `clear_bit` updates the corresponding word. Words that become all-zero are removed from storage.

## Bitwise Helpers

`Uint256` in cosmwasm-std 2.2.2 implements `Not` and `Shl`/`Shr` but not `BitOr` or `BitAnd`. The module provides byte-level `bitor`/`bitand` helpers that convert to `[u8; 32]` LE, apply native byte ops, and convert back. `set_bit`, `clear_bit`, and `is_set` are built on these and are idempotent by construction (no guards needed).
