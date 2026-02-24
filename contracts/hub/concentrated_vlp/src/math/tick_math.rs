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

pub fn get_tick_at_sqrt_ratio(sqrt_price_x96: Uint256) -> Result<i64, ContractError> {
    if sqrt_price_x96 < min_sqrt_ratio() || sqrt_price_x96 >= max_sqrt_ratio() {
        return Err(ContractError::new("sqrt price out of bounds"));
    }

    let mut lo = MIN_TICK;
    let mut hi = MAX_TICK;
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        if get_sqrt_ratio_at_tick(mid)? <= sqrt_price_x96 {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Ok(lo)
}

#[cfg(test)]
mod tests {
    use super::{get_sqrt_ratio_at_tick, get_tick_at_sqrt_ratio, max_sqrt_ratio, min_sqrt_ratio};
    use crate::state::{MAX_TICK, MIN_TICK};

    #[test]
    fn tick_math_boundaries_match_v3_constants() {
        assert_eq!(get_sqrt_ratio_at_tick(MIN_TICK).unwrap(), min_sqrt_ratio());
        assert!(get_sqrt_ratio_at_tick(MAX_TICK).unwrap() <= max_sqrt_ratio());
    }

    #[test]
    fn tick_to_sqrt_and_back_roundtrip() {
        for tick in [-120_000i64, -60, -1, 0, 1, 60, 120_000, MAX_TICK - 1] {
            let sqrt = get_sqrt_ratio_at_tick(tick).unwrap();
            let resolved = get_tick_at_sqrt_ratio(sqrt).unwrap();
            assert_eq!(resolved, tick);
        }
    }
}
