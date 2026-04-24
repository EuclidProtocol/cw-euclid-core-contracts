use cosmwasm_std::Uint128;

pub fn price_to_tick(price: f64) -> i64 {
    (price.ln() / 1.0001_f64.ln()).floor() as i64
}

pub fn pair_to_tick(amount_a: Uint128, amount_b: Uint128) -> i64 {
    price_to_tick(amount_b.u128() as f64 / amount_a.u128() as f64)
}
