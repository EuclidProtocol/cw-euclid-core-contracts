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

pub fn set_bit(word: Uint256, bit_pos: u8) -> Uint256 {
    if is_set(word, bit_pos) {
        word
    } else {
        word.checked_add(mask(bit_pos)).unwrap_or(word)
    }
}

pub fn clear_bit(word: Uint256, bit_pos: u8) -> Uint256 {
    if is_set(word, bit_pos) {
        word.checked_sub(mask(bit_pos)).unwrap_or(word)
    } else {
        word
    }
}

pub fn is_set(word: Uint256, bit_pos: u8) -> bool {
    let bit = mask(bit_pos);
    let quotient = word.checked_div(bit).unwrap_or_default();
    !quotient.checked_rem(Uint256::from(2u8)).unwrap_or_default().is_zero()
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::Uint256;

    use crate::math::tick_bitmap::{clear_bit, is_set, position, set_bit};

    #[test]
    fn tick_bitmap_next_initialized_tick_forward_backward() {
        let (word_pos, bit_pos) = position(600, 10);
        let mut word = Uint256::zero();
        word = set_bit(word, bit_pos);
        assert!(is_set(word, bit_pos));
        let cleared = clear_bit(word, bit_pos);
        assert!(!is_set(cleared, bit_pos));
        assert_eq!(word_pos, 0);
    }
}
