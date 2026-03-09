use std::str::FromStr;

use cosmwasm_std::{Uint256, Uint512};
use euclid::error::ContractError;

use crate::state::{MAX_TICK, MIN_TICK};

pub const MIN_SQRT_RATIO_STR: &str = "4295128739";
pub const MAX_SQRT_RATIO_STR: &str = "1461446703485210103287273052203988822378723970342";

fn from_hex(hex: &str) -> Uint256 {
    let raw = hex.strip_prefix("0x").unwrap_or(hex);
    let mut bytes = [0u8; 32];
    let mut i = raw.len();
    let mut out = 32usize;
    while i > 0 {
        let start = i.saturating_sub(2);
        let chunk = &raw[start..i];
        out -= 1;
        bytes[out] = u8::from_str_radix(chunk, 16).expect("invalid tick math hex constant");
        i = start;
    }
    Uint256::from_be_bytes(bytes)
}

fn mul_shift_128(a: Uint256, b: Uint256) -> Result<Uint256, ContractError> {
    let prod = Uint512::from(a).checked_mul(Uint512::from(b))?;
    let shifted = prod >> 128u32;
    Uint256::try_from(shifted).map_err(|_| ContractError::new("tick mul shift overflow"))
}

pub fn min_sqrt_ratio() -> Uint256 {
    Uint256::from_str(MIN_SQRT_RATIO_STR).expect("valid min sqrt ratio")
}

pub fn max_sqrt_ratio() -> Uint256 {
    Uint256::from_str(MAX_SQRT_RATIO_STR).expect("valid max sqrt ratio")
}

pub fn get_sqrt_ratio_at_tick(tick: i64) -> Result<Uint256, ContractError> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return Err(ContractError::new("tick out of bounds"));
    }

    let abs_tick = tick.unsigned_abs();
    let mut ratio = if (abs_tick & 0x1) != 0 {
        from_hex("0xfffcb933bd6fad37aa2d162d1a594001")
    } else {
        Uint256::one() << 128u32
    };

    if (abs_tick & 0x2) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xfff97272373d413259a46990580e213a"))?;
    }
    if (abs_tick & 0x4) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xfff2e50f5f656932ef12357cf3c7fdcc"))?;
    }
    if (abs_tick & 0x8) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xffe5caca7e10e4e61c3624eaa0941cd0"))?;
    }
    if (abs_tick & 0x10) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xffcb9843d60f6159c9db58835c926644"))?;
    }
    if (abs_tick & 0x20) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xff973b41fa98c081472e6896dfb254c0"))?;
    }
    if (abs_tick & 0x40) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xff2ea16466c96a3843ec78b326b52861"))?;
    }
    if (abs_tick & 0x80) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xfe5dee046a99a2a811c461f1969c3053"))?;
    }
    if (abs_tick & 0x100) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xfcbe86c7900a88aedcffc83b479aa3a4"))?;
    }
    if (abs_tick & 0x200) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xf987a7253ac413176f2b074cf7815e54"))?;
    }
    if (abs_tick & 0x400) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xf3392b0822b70005940c7a398e4b70f3"))?;
    }
    if (abs_tick & 0x800) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xe7159475a2c29b7443b29c7fa6e889d9"))?;
    }
    if (abs_tick & 0x1000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xd097f3bdfd2022b8845ad8f792aa5825"))?;
    }
    if (abs_tick & 0x2000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0xa9f746462d870fdf8a65dc1f90e061e5"))?;
    }
    if (abs_tick & 0x4000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0x70d869a156d2a1b890bb3df62baf32f7"))?;
    }
    if (abs_tick & 0x8000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0x31be135f97d08fd981231505542fcfa6"))?;
    }
    if (abs_tick & 0x10000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0x9aa508b5b7a84e1c677de54f3e99bc9"))?;
    }
    if (abs_tick & 0x20000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0x5d6af8dedb81196699c329225ee604"))?;
    }
    if (abs_tick & 0x40000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0x2216e584f5fa1ea926041bedfe98"))?;
    }
    if (abs_tick & 0x80000) != 0 {
        ratio = mul_shift_128(ratio, from_hex("0x48a170391f7dc42444e8fa2"))?;
    }

    if tick > 0 {
        ratio = Uint256::MAX.checked_div(ratio)?;
    }

    let round_mask = (Uint256::one() << 32u32).checked_sub(Uint256::one())?;
    let mut sqrt_price_x96 = ratio >> 32u32;
    if !ratio.checked_rem(round_mask.checked_add(Uint256::one())?)?.is_zero() {
        sqrt_price_x96 = sqrt_price_x96.checked_add(Uint256::one())?;
    }

    Ok(sqrt_price_x96)
}

/// Returns the most significant bit position of a Uint256 (0-indexed).
/// Returns 0 for input 0.
fn most_significant_bit(x: Uint256) -> u16 {
    let bytes = x.to_be_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != 0 {
            return ((31 - i) * 8 + 7 - b.leading_zeros() as usize) as u16;
        }
    }
    0
}

/// Two's complement signed 256-bit integer, stored as a raw Uint256.
/// This matches Solidity's int256 behavior exactly.
#[derive(Clone, Copy)]
struct I256(Uint256);

/// The sign bit: 2^255
fn sign_bit() -> Uint256 {
    Uint256::one() << 255u32
}

impl I256 {
    fn from_signed(val: i128) -> Self {
        if val >= 0 {
            I256(Uint256::from(val as u128))
        } else {
            // Two's complement: -x = NOT(x) + 1 = (2^256 - 1 - x) + 1 = 2^256 - x
            // But Uint256 can't hold 2^256, so: wrapping_sub from 0
            // In Uint256: 0 - magnitude wraps to 2^256 - magnitude
            let magnitude = Uint256::from(val.unsigned_abs());
            I256(Uint256::zero().wrapping_sub(magnitude))
        }
    }

    fn is_negative(&self) -> bool {
        self.0 >= sign_bit()
    }

    /// Left shift (preserving two's complement semantics).
    fn shl(self, n: u32) -> Self {
        I256(self.0 << n)
    }

    /// Add a Uint256 value (wrapping).
    fn add_uint(self, rhs: Uint256) -> Self {
        I256(self.0.wrapping_add(rhs))
    }

    /// Wrapping multiply: compute self * rhs, wrapping the result to 256 bits.
    /// Handles sign correctly: multiply absolute values, negate if signs differ.
    fn wrapping_mul(self, rhs: Self) -> Self {
        let self_neg = self.is_negative();
        let rhs_neg = rhs.is_negative();
        let result_neg = self_neg ^ rhs_neg;

        // Get absolute values
        let a = if self_neg { Uint256::zero().wrapping_sub(self.0) } else { self.0 };
        let b = if rhs_neg { Uint256::zero().wrapping_sub(rhs.0) } else { rhs.0 };

        // Multiply in Uint512 for full precision, truncate to 256 bits
        let product = Uint512::from(a)
            .checked_mul(Uint512::from(b))
            .unwrap_or(Uint512::zero());
        let lo_bytes = product.to_le_bytes();
        let mut result_bytes = [0u8; 32];
        result_bytes.copy_from_slice(&lo_bytes[..32]);
        let magnitude = Uint256::from_le_bytes(result_bytes);

        if result_neg && !magnitude.is_zero() {
            // Negate: two's complement
            I256(Uint256::zero().wrapping_sub(magnitude))
        } else {
            I256(magnitude)
        }
    }

    /// Wrapping subtraction.
    fn wrapping_sub(self, rhs: Self) -> Self {
        I256(self.0.wrapping_sub(rhs.0))
    }

    /// Wrapping addition.
    fn wrapping_add(self, rhs: Self) -> Self {
        I256(self.0.wrapping_add(rhs.0))
    }

    /// Arithmetic shift right by n bits, returning i64.
    /// In two's complement, arithmetic right shift sign-extends.
    fn asr(self, n: u32) -> i64 {
        if !self.is_negative() {
            // Non-negative: just shift right
            let shifted = self.0 >> n;
            let bytes = shifted.to_le_bytes();
            u64::from_le_bytes(bytes[0..8].try_into().expect("8 bytes")) as i64
        } else {
            // Negative two's complement: negate, shift, negate result.
            // -x in two's complement: NOT(x) + 1
            let magnitude = Uint256::zero().wrapping_sub(self.0); // = |value|
            let shifted_mag = magnitude >> n;
            let bytes = shifted_mag.to_le_bytes();
            let val = u64::from_le_bytes(bytes[0..8].try_into().expect("8 bytes")) as i64;
            // For arithmetic shift right of negative numbers, round toward -inf.
            // Check if any bits were shifted out.
            let remainder = magnitude.wrapping_sub(shifted_mag << n);
            if remainder > Uint256::zero() {
                -val - 1
            } else {
                -val
            }
        }
    }
}

/// O(1) computation of tick from sqrt price, ported from Uniswap V3's TickMath.sol.
///
/// Computes log_{sqrt(1.0001)}(sqrt_price_x96 / 2^96) using fixed-point log2,
/// then converts to tick space via multiplication by log_{sqrt(1.0001)}(2).
pub fn get_tick_at_sqrt_ratio(sqrt_price_x96: Uint256) -> Result<i64, ContractError> {
    if sqrt_price_x96 < min_sqrt_ratio() || sqrt_price_x96 >= max_sqrt_ratio() {
        return Err(ContractError::new("sqrt price out of bounds"));
    }

    let ratio_x128 = sqrt_price_x96 << 32u32;
    let msb = most_significant_bit(ratio_x128);

    // Normalize r into [1, 2) range in Q127 fixed-point.
    let r = if msb >= 128 {
        ratio_x128 >> (msb - 127) as u32
    } else {
        ratio_x128 << (127 - msb) as u32
    };

    // Build log2 as a Q64 fixed-point value (matching V3's `(msb - 128) << 64`).
    let mut log2 = I256::from_signed(msb as i128 - 128).shl(64);

    let mut r = r;

    // Compute 14 fractional bits by repeated squaring (bits 63 down to 50).
    macro_rules! log2_bit {
        ($bit:expr) => {{
            let sq = Uint512::from(r).checked_mul(Uint512::from(r))?;
            r = Uint256::try_from(sq >> 127u32)
                .map_err(|_| ContractError::new("log2 overflow"))?;
            let f = r >> 128u32;
            log2 = log2.add_uint(f << $bit);
            r >>= f.to_le_bytes()[0] as u32;
        }};
    }

    log2_bit!(63u32);
    log2_bit!(62u32);
    log2_bit!(61u32);
    log2_bit!(60u32);
    log2_bit!(59u32);
    log2_bit!(58u32);
    log2_bit!(57u32);
    log2_bit!(56u32);
    log2_bit!(55u32);
    log2_bit!(54u32);
    log2_bit!(53u32);
    log2_bit!(52u32);
    log2_bit!(51u32);
    log2_bit!(50u32);

    // Convert log2 (Q64) to log_{sqrt(1.0001)}.
    // V3: log_sqrt10001 = log_2 * 255738958999603826347141 (result is "128.128 number")
    // The product is Q64 * ~78-bit integer. Since V3 uses wrapping int256 multiply,
    // and the result is described as 128.128, the effective precision is in the
    // low bits. We use wrapping multiply to match V3.
    let log_sqrt10001 = log2.wrapping_mul(
        I256(Uint256::from(255738958999603826347141u128)),
    );

    // V3 error margins (int256 constants, both positive, in the same Q representation):
    //   low:  3402992956809132418596140100660247210
    //   high: 291339464771989622907027621153398088495
    let err_low = I256(
        Uint256::from_str("3402992956809132418596140100660247210")
            .expect("valid constant"),
    );
    let err_high = I256(
        Uint256::from_str("291339464771989622907027621153398088495")
            .expect("valid constant"),
    );

    // V3: tickLow = int24((log_sqrt10001 - err_low) >> 128)
    // V3: tickHi  = int24((log_sqrt10001 + err_high) >> 128)
    let tick_low = log_sqrt10001.wrapping_sub(err_low).asr(128);
    let tick_high = log_sqrt10001.wrapping_add(err_high).asr(128);

    if tick_low == tick_high {
        Ok(tick_low)
    } else if get_sqrt_ratio_at_tick(tick_high)? <= sqrt_price_x96 {
        Ok(tick_high)
    } else {
        Ok(tick_low)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cosmwasm_std::Uint256;

    use super::{get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, max_sqrt_ratio, min_sqrt_ratio};
    use crate::state::{MAX_TICK, MIN_TICK};

    #[test]
    fn tick_math_boundaries_match_v3_constants() {
        assert_eq!(get_sqrt_ratio_at_tick(MIN_TICK).unwrap(), min_sqrt_ratio());
        assert!(get_sqrt_ratio_at_tick(MAX_TICK).unwrap() <= max_sqrt_ratio());
    }

    /// Table-driven test: tick → sqrt_price, verified against captured reference values.
    #[test]
    fn get_sqrt_ratio_at_tick_reference_vectors() {
        let cases: &[(i64, &str)] = &[
            (MIN_TICK, "4295128739"),
            (-500000, "1101692437043807371"),
            (-200000, "3598751819609688046946419"),
            (-100000, "533968626430936354154228408"),
            (-50000, "6504256538020985011912221507"),
            (-10000, "48055510970269007215549348797"),
            (-1000, "75364347830767020784054125655"),
            (-100, "78833030112140176575862854579"),
            (-10, "79188560314459151373725315960"),
            (-1, "79224201403219477170569942574"),
            (0, "79228162514264337593543950336"),
            (1, "79232123823359799118286999568"),
            (10, "79267784519130042428790663799"),
            (100, "79625275426524748796330556128"),
            (1000, "83290069058676223003182343270"),
            (10000, "130621891405341611593710811006"),
            (50000, "965075977353221155028623082916"),
            (100000, "11755562826496067164730007768450"),
            (200000, "1744244129640337381386292603617838"),
            (500000, "5697689776495288729098254600827762987878"),
            (MAX_TICK - 1, "1461373636630004318706518188784493106690254656249"),
        ];

        for (tick, expected_str) in cases {
            let result = get_sqrt_ratio_at_tick(*tick).expect(&format!("tick {tick} should succeed"));
            let expected = Uint256::from_str(expected_str).expect("valid decimal");
            assert_eq!(
                result, expected,
                "get_sqrt_ratio_at_tick({tick}): got {result}, expected {expected}"
            );
        }
    }

    /// Table-driven roundtrip: tick → sqrt_price → tick, verifying identity.
    #[test]
    fn tick_to_sqrt_and_back_roundtrip() {
        let ticks: &[i64] = &[
            MIN_TICK,
            -500000,
            -200000,
            -120000,
            -50000,
            -10000,
            -1000,
            -100,
            -60,
            -10,
            -1,
            0,
            1,
            10,
            60,
            100,
            1000,
            10000,
            50000,
            120000,
            200000,
            500000,
            MAX_TICK - 1,
        ];

        for tick in ticks {
            let sqrt = get_sqrt_ratio_at_tick(*tick).expect(&format!("tick {tick} forward"));
            let resolved = get_tick_at_sqrt_ratio(sqrt).expect(&format!("tick {tick} inverse"));
            assert_eq!(resolved, *tick, "roundtrip failed for tick {tick}");
        }
    }

    /// Table-driven test for get_tick_at_sqrt_ratio with known sqrt prices.
    /// Tests prices that fall between ticks to verify floor behavior.
    #[test]
    fn get_tick_at_sqrt_ratio_reference_vectors() {
        let cases: &[(&str, i64)] = &[
            // Exact tick boundaries
            ("4295128739", MIN_TICK),
            ("79228162514264337593543950336", 0),
            // Between ticks — should floor to lower tick
            ("79228162514264337593543950337", 0),
            ("79232123823359799118286999567", 0),   // one below tick 1
            ("79232123823359799118286999568", 1),   // exact tick 1
            // Various magnitudes
            ("533968626430936354154228408", -100000),
            ("11755562826496067164730007768450", 100000),
        ];

        for (sqrt_str, expected_tick) in cases {
            let sqrt = Uint256::from_str(sqrt_str).expect("valid decimal");
            let result = get_tick_at_sqrt_ratio(sqrt).expect(&format!("sqrt {sqrt_str} should succeed"));
            assert_eq!(
                result, *expected_tick,
                "get_tick_at_sqrt_ratio({sqrt_str}): got {result}, expected {expected_tick}"
            );
        }
    }

    /// Boundary error cases.
    #[test]
    fn get_tick_at_sqrt_ratio_rejects_out_of_bounds() {
        let cases: &[(&str, &str)] = &[
            ("0", "below min"),
            ("4295128738", "one below min"),
            ("1461446703485210103287273052203988822378723970342", "at max (exclusive)"),
        ];

        for (sqrt_str, label) in cases {
            let sqrt = Uint256::from_str(sqrt_str).expect("valid decimal");
            assert!(
                get_tick_at_sqrt_ratio(sqrt).is_err(),
                "should reject {label}: {sqrt_str}"
            );
        }
    }

    #[test]
    fn get_sqrt_ratio_at_tick_rejects_out_of_bounds() {
        assert!(get_sqrt_ratio_at_tick(MIN_TICK - 1).is_err());
        assert!(get_sqrt_ratio_at_tick(MAX_TICK + 1).is_err());
    }
}
