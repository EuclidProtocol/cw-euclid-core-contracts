use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct UsageFee {
    pub fee_recipient: String,
    pub free_limit: u128,
    pub linear_limit: u128,
    pub base_fee: Uint128,
    pub slope_fee: Uint128,
    pub quadk_fee: Uint128,
    pub max_fee: Uint128,
}

impl UsageFee {
    pub fn default() -> Self {
        Self {
            fee_recipient: "fee_recipient".to_string(),
            free_limit: 0,
            linear_limit: 0,
            base_fee: Uint128::zero(),
            slope_fee: Uint128::zero(),
            quadk_fee: Uint128::zero(),
            max_fee: Uint128::zero(),
        }
    }
}

pub fn calc_fee(config: &UsageFee, count: u128) -> Uint128 {
    let base = config.base_fee;
    let slope = config.slope_fee;
    let quadk = config.quadk_fee;
    let cap = config.max_fee;
    let free_limit = config.free_limit;
    let linear_limit = config.linear_limit;

    if count <= free_limit {
        Uint128::zero()
    } else if count <= linear_limit {
        // Linear growth: base + slope * (count - free_limit)
        let excess = count - free_limit;
        let fee = base + slope * Uint128::from(excess);
        if fee > cap {
            cap
        } else {
            fee
        }
    } else {
        // Hybrid: linear plateau + quadratic beyond linear_limit
        let excess = count - linear_limit;
        let fee = base + slope * Uint128::from(10_u128) + quadk * Uint128::from(excess * excess);
        if fee > cap {
            cap
        } else {
            fee
        }
    }
}
