use std::str::FromStr;

use cosmwasm_std::{Int256, Uint256, Uint512};
use euclid::error::ContractError;

use crate::state::{MAX_TICK, MIN_TICK};

pub const MIN_SQRT_RATIO_STR: &str = "4295128739";
pub const MAX_SQRT_RATIO_STR: &str = "1461446703485210103287273052203988822378723970342";

/// 1 / log2(sqrt(1.0001)), scaled for Q64 × integer → Q128 multiplication.
const LOG_SQRT10001_SCALE: u128 = 255738958999603826347141;
/// Lower error margin for tick resolution (V3 precomputed constant).
const TICK_LOW_ERR: &str = "3402992956809132418596140100660247210";
/// Upper error margin for tick resolution (V3 precomputed constant).
const TICK_HIGH_ERR: &str = "291339464771989622907027621153398088495";

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

/// Converts a tick index to its corresponding Q96 fixed-point square-root price.
///
/// Each tick `t` represents the price ratio `1.0001^t`, so:
///
///   sqrt_price = sqrt(1.0001^t) * 2^96
///
/// The implementation avoids floating-point by decomposing `|t|` into its binary
/// bits and multiplying in a chain of precomputed Q128 constants — one constant
/// per bit position — each equal to `sqrt(1.0001^(2^i))` in Q128. After the
/// chain multiply the result is shifted right by 32 bits (Q128 → Q96) and
/// rounded up. For negative ticks the final ratio is inverted via `MAX / ratio`.
///
/// Valid range: `MIN_TICK..=MAX_TICK`. Returns an error outside that range.
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
    if !ratio
        .checked_rem(round_mask.checked_add(Uint256::one())?)?
        .is_zero()
    {
        sqrt_price_x96 = sqrt_price_x96.checked_add(Uint256::one())?;
    }

    Ok(sqrt_price_x96)
}

/// Returns the position of the most significant set bit (0-indexed from LSB).
/// For example: msb(1) = 0, msb(8) = 3, msb(2^128) = 128.
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

/// Compute log2(sqrt_price_x96) as a Q64 fixed-point Int256.
/// See docs/tick_math_algorithm.md for a full walkthrough of the math.
fn log2_q64(sqrt_price_x96: Uint256) -> Result<Int256, ContractError> {
    // Step 1a: Integer part of log2 — convert Q96 → Q128, MSB gives floor(log2)
    let ratio_x128 = sqrt_price_x96 << 32u32;
    let msb = most_significant_bit(ratio_x128);

    // Step 1b: Normalize r to [1.0, 2.0) in Q127 for fractional bit extraction
    let mut r = if msb >= 128 {
        ratio_x128 >> (msb - 127) as u32
    } else {
        ratio_x128 << (127 - msb) as u32
    };

    let mut log2 = Int256::from(msb as i128 - 128) << 64u32;

    // Extract 14 fractional bits via repeated squaring (see docs for proof)
    for bit in (50..=63u32).rev() {
        let sq = Uint512::from(r).checked_mul(Uint512::from(r))?;
        r = Uint256::try_from(sq >> 127u32).map_err(|_| ContractError::new("log2 overflow"))?;
        let f = r >> 128u32; // 1 if r >= 2.0, else 0
        let frac_bit = Int256::try_from(f << bit)
            .map_err(|_| ContractError::new("log2 fractional bit overflow"))?;
        log2 = log2.wrapping_add(frac_bit);
        r >>= f.to_le_bytes()[0] as u32; // divide by 2 if f == 1
    }

    Ok(log2)
}

/// Converts a Q96 fixed-point square-root price back to the largest tick `t`
/// such that `get_sqrt_ratio_at_tick(t) <= sqrt_price_x96` (i.e. floor toward
/// lower tick).
///
/// To find the tick from a price:
///
///   t = floor( log_{sqrt(1.0001)}( sqrt_price_x96 / 2^96 ) )
///     = floor( log2(sqrt_price_x96 / 2^96) / log2(sqrt(1.0001)) )
///
/// Steps:
///   1. Compute `log2(sqrt_price_x96)` as a Q64 fixed-point integer using
///      the MSB for the integer part and 14 rounds of repeated squaring for
///      the fractional bits (`log2_q64`).
///   2. Multiply by `1 / log2(sqrt(1.0001))` (the precomputed Q64 constant
///      `LOG_SQRT10001_SCALE`) to change the base, yielding a Q128 tick.
///   3. Shift right 128 bits to get candidate ticks `tick_low` and `tick_high`
///      (differing by ±1) after applying precomputed error margins that account
///      for rounding in step 1.
///   4. Verify by calling `get_sqrt_ratio_at_tick(tick_high)` and return the
///      correct floor tick.
///
/// Ported from Uniswap V3's TickMath.sol.
pub fn get_tick_at_sqrt_ratio(sqrt_price_x96: Uint256) -> Result<i64, ContractError> {
    if sqrt_price_x96 < min_sqrt_ratio() || sqrt_price_x96 >= max_sqrt_ratio() {
        return Err(ContractError::new("sqrt price out of bounds"));
    }

    // Step 1: log2(sqrt_price) as Q64
    let log2 = log2_q64(sqrt_price_x96)?;

    // Step 2: Change of base — log2 * (1 / log2(sqrt(1.0001))) → Q128 tick
    let sqrt_price_base = Int256::from(LOG_SQRT10001_SCALE as i128);
    let log_sqrt10001 = log2.wrapping_mul(sqrt_price_base);

    // Step 3: Resolve ±1 rounding via precomputed error margins
    let err_low = Int256::from_str(TICK_LOW_ERR).expect("valid constant");
    let err_high = Int256::from_str(TICK_HIGH_ERR).expect("valid constant");

    let tick_low: i64 = i64::from(
        cosmwasm_std::Int64::try_from(log_sqrt10001.wrapping_sub(err_low) >> 128u32)
            .map_err(|e| ContractError::new(&e.to_string()))?,
    );
    let tick_high: i64 = i64::from(
        cosmwasm_std::Int64::try_from(log_sqrt10001.wrapping_add(err_high) >> 128u32)
            .map_err(|e| ContractError::new(&e.to_string()))?,
    );

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

    use cosmwasm_std::{Int256, Uint256};

    use super::{
        get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, log2_q64, max_sqrt_ratio, min_sqrt_ratio,
        most_significant_bit,
    };
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
            (
                MAX_TICK - 1,
                "1461373636630004318706518188784493106690254656249",
            ),
        ];

        for (tick, expected_str) in cases {
            let result = get_sqrt_ratio_at_tick(*tick)
                .unwrap_or_else(|_| panic!("tick {tick} should succeed"));
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
            let sqrt =
                get_sqrt_ratio_at_tick(*tick).unwrap_or_else(|_| panic!("tick {tick} forward"));
            let resolved =
                get_tick_at_sqrt_ratio(sqrt).unwrap_or_else(|_| panic!("tick {tick} inverse"));
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
            ("79232123823359799118286999567", 0), // one below tick 1
            ("79232123823359799118286999568", 1), // exact tick 1
            // Various magnitudes
            ("533968626430936354154228408", -100000),
            ("11755562826496067164730007768450", 100000),
        ];

        for (sqrt_str, expected_tick) in cases {
            let sqrt = Uint256::from_str(sqrt_str).expect("valid decimal");
            let result = get_tick_at_sqrt_ratio(sqrt)
                .unwrap_or_else(|_| panic!("sqrt {sqrt_str} should succeed"));
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
            (
                "1461446703485210103287273052203988822378723970342",
                "at max (exclusive)",
            ),
        ];

        for (sqrt_str, label) in cases {
            let sqrt = Uint256::from_str(sqrt_str).expect("valid decimal");
            assert!(
                get_tick_at_sqrt_ratio(sqrt).is_err(),
                "should reject {label}: {sqrt_str}"
            );
        }
    }

    /// Table-driven test for log2_q64: verifies the Q64 fixed-point log2 output
    /// for known sqrt prices.
    #[test]
    fn log2_q64_reference_vectors() {
        let cases: &[(&str, &str)] = &[
            // tick 0: log2 = 0
            ("79228162514264337593543950336", "0"),
            // tick 1: small positive
            ("79232123823359799118286999568", "1125899906842624"),
            // tick 10
            ("79267784519130042428790663799", "12384898975268864"),
            // tick 1000
            ("83290069058676223003182343270", "1329687789981138944"),
            // tick 10000
            ("130621891405341611593710811006", "13304759199159287808"),
            // tick 100000
            ("11755562826496067164730007768450", "133057725090754461696"),
            // tick -100000
            ("533968626430936354154228408", "-133058850990661304320"),
            // tick -1000
            ("75364347830767020784054125655", "-1330813689887981568"),
            // MIN_TICK
            ("4295128739", "-1180591620717411303424"),
        ];

        for (sqrt_str, expected_str) in cases {
            let sqrt = Uint256::from_str(sqrt_str).expect("valid decimal");
            let result =
                log2_q64(sqrt).unwrap_or_else(|_| panic!("log2_q64({sqrt_str}) should succeed"));
            let expected = Int256::from_str(expected_str).expect("valid decimal");
            assert_eq!(
                result, expected,
                "log2_q64({sqrt_str}): got {result}, expected {expected}"
            );
        }
    }

    #[test]
    fn most_significant_bit_reference_vectors() {
        let cases: &[(&str, u16)] = &[
            ("0", 0),
            ("1", 0),
            ("2", 1),
            ("3", 1),
            ("8", 3),
            ("255", 7),
            ("256", 8),
            // Powers of 2
            ("65536", 16),                                    // 2^16
            ("4294967296", 32),                               // 2^32
            ("18446744073709551616", 64),                     // 2^64
            ("340282366920938463463374607431768211456", 128), // 2^128
        ];

        for (val_str, expected_msb) in cases {
            let val = Uint256::from_str(val_str).expect("valid decimal");
            assert_eq!(
                most_significant_bit(val),
                *expected_msb,
                "msb({val_str}): expected {expected_msb}"
            );
        }
    }

    #[test]
    fn get_sqrt_ratio_at_tick_rejects_out_of_bounds() {
        assert!(get_sqrt_ratio_at_tick(MIN_TICK - 1).is_err());
        assert!(get_sqrt_ratio_at_tick(MAX_TICK + 1).is_err());
    }
}
