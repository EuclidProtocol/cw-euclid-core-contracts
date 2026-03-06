//! Tick bitmap for efficient next-initialized-tick lookup during swaps.
//! See `TICK_BITMAP.md` in the contract root for the full design doc.

use cosmwasm_std::Uint256;

fn floor_div(a: i64, b: i64) -> i64 {
    let q = a / b;
    let r = a % b;
    if r != 0 && ((r > 0) != (b > 0)) {
        q - 1
    } else {
        q
    }
}

pub fn position(tick: i64, tick_spacing: u64) -> (i64, u8) {
    let compressed = floor_div(tick, tick_spacing as i64);
    let word_pos = floor_div(compressed, 256);
    let bit_pos = compressed.rem_euclid(256) as u8;
    (word_pos, bit_pos)
}

pub fn mask(bit_pos: u8) -> Uint256 {
    Uint256::one() << (bit_pos as u32)
}

/// Bytewise OR — cosmwasm-std 2.2.2 implements `Not` and `Shl`/`Shr` for Uint256
/// but not `BitOr` or `BitAnd`, so we operate on the LE byte representation.
fn bitor(a: Uint256, b: Uint256) -> Uint256 {
    let a_bytes = a.to_le_bytes();
    let b_bytes = b.to_le_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = a_bytes[i] | b_bytes[i];
    }
    Uint256::from_le_bytes(out)
}

fn bitand(a: Uint256, b: Uint256) -> Uint256 {
    let a_bytes = a.to_le_bytes();
    let b_bytes = b.to_le_bytes();
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = a_bytes[i] & b_bytes[i];
    }
    Uint256::from_le_bytes(out)
}

pub fn set_bit(word: Uint256, bit_pos: u8) -> Uint256 {
    bitor(word, mask(bit_pos))
}

pub fn clear_bit(word: Uint256, bit_pos: u8) -> Uint256 {
    bitand(word, !mask(bit_pos))
}

pub fn is_set(word: Uint256, bit_pos: u8) -> bool {
    !bitand(word, mask(bit_pos)).is_zero()
}

/// Find the next initialized (set) bit in a word, searching from `from_bit` inclusive.
/// If `lte` is true, searches downward (descending); otherwise upward (ascending).
/// Returns `None` if no set bit is found in the search direction.
///
/// Uses a byte-level scan (at most 32 byte checks + 8 bit checks) rather than
/// a bit-level scan (up to 256 `Uint256` operations).
pub fn next_initialized_bit_in_word(word: Uint256, from_bit: u8, lte: bool) -> Option<u8> {
    let bytes = word.to_le_bytes(); // single conversion, LE: bytes[0] has bits 0-7

    if lte {
        // Search from from_bit downward to 0
        let start_byte = (from_bit / 8) as usize;
        for byte_idx in (0..=start_byte).rev() {
            let b = bytes[byte_idx];
            if b == 0 {
                continue;
            }
            // Mask out bits above from_bit within the start byte
            let masked = if byte_idx == start_byte {
                let bits_to_keep = (from_bit % 8) + 1; // keep bits 0..=from_bit%8
                if bits_to_keep == 8 { b } else { b & ((1u8 << bits_to_keep) - 1) }
            } else {
                b
            };
            if masked == 0 {
                continue;
            }
            // Highest set bit in masked byte
            let bit_in_byte = 7 - masked.leading_zeros() as u8;
            return Some(byte_idx as u8 * 8 + bit_in_byte);
        }
        None
    } else {
        // Search from from_bit upward to 255
        let start_byte = (from_bit / 8) as usize;
        for byte_idx in start_byte..32 {
            let b = bytes[byte_idx];
            if b == 0 {
                continue;
            }
            // Mask out bits below from_bit within the start byte
            let masked = if byte_idx == start_byte {
                let shift = from_bit % 8;
                b >> shift << shift // clear bits below from_bit%8
            } else {
                b
            };
            if masked == 0 {
                continue;
            }
            // Lowest set bit in masked byte
            let bit_in_byte = masked.trailing_zeros() as u8;
            return Some(byte_idx as u8 * 8 + bit_in_byte);
        }
        None
    }
}

/// Reconstruct the tick value from a bitmap word position and bit position.
/// Inverse of `position()`.
pub fn tick_from_word_and_bit(word_pos: i64, bit_pos: u8, tick_spacing: u64) -> i64 {
    let compressed = word_pos * 256 + (bit_pos as i64);
    compressed * (tick_spacing as i64)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::Uint256;

    use crate::math::tick_bitmap::{clear_bit, is_set, next_initialized_bit_in_word, position, set_bit, tick_from_word_and_bit};

    #[test]
    fn bitor_cases() {
        use super::{bitor, bitand};

        struct Case { a: Uint256, b: Uint256, expected: Uint256, name: &'static str }
        let cases = [
            Case {
                a: Uint256::zero(),
                b: Uint256::zero(),
                expected: Uint256::zero(),
                name: "zero | zero",
            },
            Case {
                a: Uint256::one(),
                b: Uint256::zero(),
                expected: Uint256::one(),
                name: "one | zero",
            },
            Case {
                a: Uint256::one(),
                b: Uint256::one(),
                expected: Uint256::one(),
                name: "one | one (idempotent)",
            },
            Case {
                a: Uint256::one(),
                b: Uint256::one() << 255u32,
                expected: {
                    // bit 0 and bit 255 both set
                    let mut bytes = [0u8; 32];
                    bytes[0] = 1;
                    bytes[31] = 128;
                    Uint256::from_le_bytes(bytes)
                },
                name: "bit 0 | bit 255",
            },
            Case {
                a: Uint256::MAX,
                b: Uint256::zero(),
                expected: Uint256::MAX,
                name: "MAX | zero",
            },
            Case {
                a: Uint256::MAX,
                b: Uint256::MAX,
                expected: Uint256::MAX,
                name: "MAX | MAX",
            },
        ];
        for case in &cases {
            assert_eq!(bitor(case.a, case.b), case.expected, "bitor: {}", case.name);
        }
    }

    #[test]
    fn bitand_cases() {
        use super::bitand;

        struct Case { a: Uint256, b: Uint256, expected: Uint256, name: &'static str }
        let cases = [
            Case {
                a: Uint256::zero(),
                b: Uint256::zero(),
                expected: Uint256::zero(),
                name: "zero & zero",
            },
            Case {
                a: Uint256::one(),
                b: Uint256::zero(),
                expected: Uint256::zero(),
                name: "one & zero",
            },
            Case {
                a: Uint256::one(),
                b: Uint256::one(),
                expected: Uint256::one(),
                name: "one & one",
            },
            Case {
                a: Uint256::one(),
                b: Uint256::one() << 255u32,
                expected: Uint256::zero(),
                name: "bit 0 & bit 255 (disjoint)",
            },
            Case {
                a: Uint256::MAX,
                b: Uint256::one() << 128u32,
                expected: Uint256::one() << 128u32,
                name: "MAX & single bit",
            },
            Case {
                a: Uint256::MAX,
                b: !Uint256::one(),
                expected: !Uint256::one(),
                name: "MAX & NOT(1) clears bit 0",
            },
        ];
        for case in &cases {
            assert_eq!(bitand(case.a, case.b), case.expected, "bitand: {}", case.name);
        }
    }

    #[test]
    fn set_clear_roundtrip() {
        let (word_pos, bit_pos) = position(600, 10);
        let mut word = Uint256::zero();
        word = set_bit(word, bit_pos);
        assert!(is_set(word, bit_pos));
        let cleared = clear_bit(word, bit_pos);
        assert!(!is_set(cleared, bit_pos));
        assert_eq!(word_pos, 0);
    }

    // L-3: set_bit and clear_bit must be idempotent (bitwise, not arithmetic).
    #[test]
    fn set_bit_and_clear_bit_are_idempotent() {
        let cases: &[(u8, &str)] = &[
            (5, "set already-set bit"),
            (0, "set already-set bit 0"),
            (255, "set already-set bit 255"),
        ];
        for &(pos, name) in cases {
            let word = set_bit(Uint256::zero(), pos);
            let result = set_bit(word, pos);
            assert_eq!(result, word, "{}: set_bit not idempotent", name);
        }

        // clear_bit on already-clear bits
        let word = Uint256::one() << 4u32; // only bit 4
        for pos in [3u8, 5, 100, 255] {
            let result = clear_bit(word, pos);
            assert_eq!(result, word, "clear_bit({}) on unset bit changed word", pos);
        }

        // set_bit must not corrupt neighbors
        let mut word = Uint256::zero();
        for pos in [4u8, 5, 6] {
            word = set_bit(word, pos);
        }
        let result = set_bit(word, 5);
        assert_eq!(result, word, "re-setting bit 5 corrupted neighbors");
    }

    // L-3: is_set correctness at boundaries and with various bit patterns.
    #[test]
    fn is_set_cases() {
        struct Case { word: Uint256, pos: u8, expected: bool, name: &'static str }
        let one = Uint256::one();
        let cases = [
            Case { word: one, pos: 0, expected: true, name: "bit 0 set in 1" },
            Case { word: one, pos: 1, expected: false, name: "bit 1 unset in 1" },
            Case { word: one << 255u32, pos: 255, expected: true, name: "bit 255 set" },
            Case { word: one << 255u32, pos: 254, expected: false, name: "bit 254 unset when only 255 set" },
            Case { word: Uint256::MAX, pos: 0, expected: true, name: "bit 0 in MAX" },
            Case { word: Uint256::MAX, pos: 127, expected: true, name: "bit 127 in MAX" },
            Case { word: Uint256::MAX, pos: 128, expected: true, name: "bit 128 in MAX" },
            Case { word: Uint256::MAX, pos: 255, expected: true, name: "bit 255 in MAX" },
            Case { word: Uint256::zero(), pos: 0, expected: false, name: "bit 0 in zero" },
            Case { word: Uint256::zero(), pos: 255, expected: false, name: "bit 255 in zero" },
        ];
        for case in &cases {
            assert_eq!(is_set(case.word, case.pos), case.expected, "{}", case.name);
        }
    }

    // M-8: next_initialized_bit_in_word searches within a single 256-bit word.
    #[test]
    fn next_initialized_bit_in_word_cases() {
        // Build a word with bits 3, 7, 12 set
        let mut word = Uint256::zero();
        for pos in [3u8, 7, 12] {
            word = set_bit(word, pos);
        }

        struct Case { bits: Option<&'static [u8]>, from: u8, lte: bool, expected: Option<u8>, name: &'static str }
        let cases = [
            // Descending
            Case { bits: None, from: 10, lte: true, expected: Some(7), name: "desc: from 10 finds 7" },
            Case { bits: None, from: 7, lte: true, expected: Some(7), name: "desc: from 7 finds 7 (inclusive)" },
            Case { bits: None, from: 2, lte: true, expected: None, name: "desc: from 2 finds nothing" },
            Case { bits: None, from: 12, lte: true, expected: Some(12), name: "desc: from 12 finds 12" },
            // Ascending
            Case { bits: None, from: 5, lte: false, expected: Some(7), name: "asc: from 5 finds 7" },
            Case { bits: None, from: 12, lte: false, expected: Some(12), name: "asc: from 12 finds 12 (inclusive)" },
            Case { bits: None, from: 13, lte: false, expected: None, name: "asc: from 13 finds nothing" },
            Case { bits: None, from: 0, lte: false, expected: Some(3), name: "asc: from 0 finds 3" },
            // Empty word
            Case { bits: Some(&[]), from: 128, lte: true, expected: None, name: "desc: empty word" },
            Case { bits: Some(&[]), from: 0, lte: false, expected: None, name: "asc: empty word" },
            // High-bit boundary (bit 255) — exercises byte-boundary masking at top of word
            Case { bits: Some(&[200, 255]), from: 255, lte: true, expected: Some(255), name: "desc: from 255 finds 255" },
            Case { bits: Some(&[200, 255]), from: 254, lte: true, expected: Some(200), name: "desc: from 254 skips 255, finds 200" },
            Case { bits: Some(&[200, 255]), from: 255, lte: false, expected: Some(255), name: "asc: from 255 finds 255 (inclusive)" },
            Case { bits: Some(&[200, 255]), from: 201, lte: false, expected: Some(255), name: "asc: from 201 finds 255" },
            // Single bit at 0
            Case { bits: Some(&[0]), from: 0, lte: true, expected: Some(0), name: "desc: from 0 finds 0" },
            Case { bits: Some(&[0]), from: 0, lte: false, expected: Some(0), name: "asc: from 0 finds 0" },
        ];

        for case in &cases {
            let w = match case.bits {
                Some(bits) => {
                    let mut w = Uint256::zero();
                    for &b in bits { w = set_bit(w, b); }
                    w
                }
                None => word,
            };
            let result = next_initialized_bit_in_word(w, case.from, case.lte);
            assert_eq!(result, case.expected, "{}", case.name);
        }
    }

    // M-8: tick_from_word_and_bit is the inverse of position().
    #[test]
    fn tick_from_word_and_bit_roundtrips_with_position() {
        for &tick_spacing in &[1u64, 10, 60, 200] {
            for &tick in &[0i64, 60, -60, 600, -600, 12000, -12000, 887220, -887220] {
                let aligned = (tick / tick_spacing as i64) * tick_spacing as i64;
                let (word_pos, bit_pos) = position(aligned, tick_spacing);
                let reconstructed = tick_from_word_and_bit(word_pos, bit_pos, tick_spacing);
                assert_eq!(
                    reconstructed, aligned,
                    "roundtrip failed for tick={}, spacing={}",
                    aligned, tick_spacing
                );
            }
        }
    }

    // L-3: Multiple bits set/cleared independently without corruption.
    #[test]
    fn multiple_bits_set_and_cleared_independently() {
        let mut word = Uint256::zero();
        for pos in [0u8, 7, 128, 255] {
            word = set_bit(word, pos);
        }
        for pos in [0u8, 7, 128, 255] {
            assert!(is_set(word, pos), "bit {} should be set", pos);
        }
        for pos in [1u8, 6, 8, 127, 129, 254] {
            assert!(!is_set(word, pos), "bit {} should not be set", pos);
        }

        word = clear_bit(word, 7);
        word = clear_bit(word, 128);
        assert!(is_set(word, 0));
        assert!(!is_set(word, 7));
        assert!(!is_set(word, 128));
        assert!(is_set(word, 255));
    }
}
