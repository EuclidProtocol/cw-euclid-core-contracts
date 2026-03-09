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
///
/// 1a. Convert Q96 → Q128, find MSB for the integer part of log2.
/// 1b. Normalize to [1.0, 2.0) in Q127, then extract 14 fractional bits
///     via repeated squaring.
fn log2_q64(sqrt_price_x96: Uint256) -> Result<Int256, ContractError> {
    // Convert from Q96 to Q128 so the "1.0" baseline sits at bit 128.
    // The MSB position then directly gives us floor(log2):
    //   MSB = 128 → value is ~2^0 = 1.0 → log2 = 0
    //   MSB = 135 → value is ~2^7 = 128  → log2 ≈ 7
    //   MSB = 64  → value is ~2^-64      → log2 ≈ -64
    let ratio_x128 = sqrt_price_x96 << 32u32;
    let msb = most_significant_bit(ratio_x128);

    // Normalize r into the range [1.0, 2.0) in Q127 fixed-point by shifting
    // so the MSB lands at bit 127. This strips out the integer part of log2,
    // leaving only the fractional remainder to compute.
    let mut r = if msb >= 128 {
        ratio_x128 >> (msb - 127) as u32
    } else {
        ratio_x128 << (127 - msb) as u32
    };

    // Start building log2 as a Q64 fixed-point Int256. The integer part goes
    // into bits [64+], leaving bits [63..0] for the fractional part.
    let mut log2 = Int256::from(msb as i128 - 128) << 64u32;

    // Each iteration extracts one fractional bit of log2:
    //   1. Square r (doubling the exponent: if r = 2^0.3, r*r = 2^0.6)
    //   2. Check if r >= 2.0 (i.e., bit 128 is set after squaring)
    //      - If yes: this fractional bit is 1. Divide r by 2 to bring
    //        it back to [1.0, 2.0) for the next iteration.
    //      - If no: this fractional bit is 0. r is already in range.
    //
    // We compute 14 bits (positions 63 down to 50 in the Q64 representation),
    // giving ~4 decimal digits of precision — enough to narrow the tick to ±1.
    for bit in (50..=63u32).rev() {
        // Square r in Q127: (Q127 * Q127) >> 127 = Q127
        let sq = Uint512::from(r).checked_mul(Uint512::from(r))?;
        r = Uint256::try_from(sq >> 127u32).map_err(|_| ContractError::new("log2 overflow"))?;
        // f = 1 if r >= 2.0 (bit 128 set), else 0
        let f = r >> 128u32;
        // Accumulate this bit into the fractional part of log2
        let frac_bit = Int256::try_from(f << bit)
            .map_err(|_| ContractError::new("log2 fractional bit overflow"))?;
        log2 = log2.wrapping_add(frac_bit);
        // If f = 1, divide r by 2 to keep it in [1.0, 2.0)
        r >>= f.to_le_bytes()[0] as u32;
    }

    Ok(log2)
}

// ---------------------------------------------------------------------------
// get_tick_at_sqrt_ratio — O(1) inverse of get_sqrt_ratio_at_tick
// ---------------------------------------------------------------------------
//
// Given a Q96 sqrt price, computes the largest tick t where
// get_sqrt_ratio_at_tick(t) <= sqrt_price_x96.
//
// This is equivalent to t = floor(log_{sqrt(1.0001)}(price)), which we
// compute in three steps:
//
//   1. Compute log2(price) using MSB + repeated squaring
//   2. Convert to tick space: tick ≈ log2(price) / log2(sqrt(1.0001))
//   3. Resolve ±1 rounding error with one call to get_sqrt_ratio_at_tick
//
// Ported from Uniswap V3's TickMath.sol getTickAtSqrtRatio.
// ---------------------------------------------------------------------------
pub fn get_tick_at_sqrt_ratio(sqrt_price_x96: Uint256) -> Result<i64, ContractError> {
    if sqrt_price_x96 < min_sqrt_ratio() || sqrt_price_x96 >= max_sqrt_ratio() {
        return Err(ContractError::new("sqrt price out of bounds"));
    }

    // Step 1: Compute log2(sqrt_price) as Q64 fixed-point
    let log2 = log2_q64(sqrt_price_x96)?;

    // -----------------------------------------------------------------------
    // Step 2: Change of base — convert log2 to tick
    // -----------------------------------------------------------------------
    // We need: tick = log2(price) / log2(sqrt(1.0001))
    //
    // The constant 255738958999603826347141 ≈ 1 / log2(sqrt(1.0001)) scaled
    // so that the Q64 * integer product gives a Q128 result (128 integer bits
    // + 128 fractional bits).
    let log_sqrt10001 = log2.wrapping_mul(Int256::from(LOG_SQRT10001_SCALE as i128));

    // -----------------------------------------------------------------------
    // Step 3: Resolve rounding — narrow to the exact tick
    // -----------------------------------------------------------------------
    // Because we only computed 14 fractional bits, log_sqrt10001 has bounded
    // error. V3 precomputes two error margins that bracket the worst case:
    //
    //   tick_low  = (log_sqrt10001 - err_low)  >> 128   (pessimistic floor)
    //   tick_high = (log_sqrt10001 + err_high) >> 128   (optimistic floor)
    //
    // The >> 128 discards the fractional bits, giving integer ticks.
    //
    // If tick_low == tick_high, precision was sufficient — return directly.
    // Otherwise they differ by exactly 1, and we resolve by checking which
    // tick's sqrt price is actually <= the input. This costs at most one call
    // to get_sqrt_ratio_at_tick (vs ~21 in the old binary search).
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
            let result =
                get_sqrt_ratio_at_tick(*tick).expect(&format!("tick {tick} should succeed"));
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
            ("79232123823359799118286999567", 0), // one below tick 1
            ("79232123823359799118286999568", 1), // exact tick 1
            // Various magnitudes
            ("533968626430936354154228408", -100000),
            ("11755562826496067164730007768450", 100000),
        ];

        for (sqrt_str, expected_tick) in cases {
            let sqrt = Uint256::from_str(sqrt_str).expect("valid decimal");
            let result =
                get_tick_at_sqrt_ratio(sqrt).expect(&format!("sqrt {sqrt_str} should succeed"));
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
            let result = log2_q64(sqrt).expect(&format!("log2_q64({sqrt_str}) should succeed"));
            let expected = Int256::from_str(expected_str).expect("valid decimal");
            assert_eq!(
                result, expected,
                "log2_q64({sqrt_str}): got {result}, expected {expected}"
            );
        }
    }

    #[test]
    fn get_sqrt_ratio_at_tick_rejects_out_of_bounds() {
        assert!(get_sqrt_ratio_at_tick(MIN_TICK - 1).is_err());
        assert!(get_sqrt_ratio_at_tick(MAX_TICK + 1).is_err());
    }
}
