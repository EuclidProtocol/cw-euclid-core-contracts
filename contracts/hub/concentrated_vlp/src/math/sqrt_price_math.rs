use cosmwasm_std::{Uint128, Uint256};
use euclid::error::ContractError;

use crate::math::full_math::{div_rounding_up, mul_div, mul_div_rounding_up};

pub fn q96() -> Uint256 {
    Uint256::one() << 96u32
}

pub fn q128() -> Uint256 {
    Uint256::one() << 128u32
}

fn as_u256(value: Uint128) -> Uint256 {
    Uint256::from(value.u128())
}

pub fn get_amount0_delta(
    mut sqrt_ratio_a_x96: Uint256,
    mut sqrt_ratio_b_x96: Uint256,
    liquidity: Uint128,
    round_up: bool,
) -> Result<Uint256, ContractError> {
    if sqrt_ratio_a_x96 > sqrt_ratio_b_x96 {
        (sqrt_ratio_a_x96, sqrt_ratio_b_x96) = (sqrt_ratio_b_x96, sqrt_ratio_a_x96);
    }
    if sqrt_ratio_a_x96.is_zero() {
        return Err(ContractError::new("sqrt ratio is zero"));
    }
    let numerator1 = as_u256(liquidity) << 96u32;
    let numerator2 = sqrt_ratio_b_x96.checked_sub(sqrt_ratio_a_x96)?;
    if round_up {
        let intermediate = mul_div_rounding_up(numerator1, numerator2, sqrt_ratio_b_x96)?;
        div_rounding_up(intermediate, sqrt_ratio_a_x96)
    } else {
        let intermediate = mul_div(numerator1, numerator2, sqrt_ratio_b_x96)?;
        intermediate.checked_div(sqrt_ratio_a_x96).map_err(ContractError::from)
    }
}

pub fn get_amount1_delta(
    mut sqrt_ratio_a_x96: Uint256,
    mut sqrt_ratio_b_x96: Uint256,
    liquidity: Uint128,
    round_up: bool,
) -> Result<Uint256, ContractError> {
    if sqrt_ratio_a_x96 > sqrt_ratio_b_x96 {
        (sqrt_ratio_a_x96, sqrt_ratio_b_x96) = (sqrt_ratio_b_x96, sqrt_ratio_a_x96);
    }
    let delta = sqrt_ratio_b_x96.checked_sub(sqrt_ratio_a_x96)?;
    if round_up {
        mul_div_rounding_up(as_u256(liquidity), delta, q96())
    } else {
        mul_div(as_u256(liquidity), delta, q96())
    }
}

pub fn get_next_sqrt_price_from_input(
    sqrt_price_x96: Uint256,
    liquidity: Uint128,
    amount_in: Uint256,
    zero_for_one: bool,
) -> Result<Uint256, ContractError> {
    if amount_in.is_zero() {
        return Ok(sqrt_price_x96);
    }
    let liquidity_u256 = as_u256(liquidity);
    if liquidity_u256.is_zero() {
        return Err(ContractError::new("zero liquidity"));
    }
    if zero_for_one {
        let product = mul_div(amount_in, sqrt_price_x96, q96())?;
        let denominator = liquidity_u256.checked_add(product)?;
        mul_div_rounding_up(liquidity_u256, sqrt_price_x96, denominator)
    } else {
        let delta = mul_div(amount_in, q96(), liquidity_u256)?;
        sqrt_price_x96.checked_add(delta).map_err(ContractError::from)
    }
}

pub fn get_next_sqrt_price_from_output(
    sqrt_price_x96: Uint256,
    liquidity: Uint128,
    amount_out: Uint256,
    zero_for_one: bool,
) -> Result<Uint256, ContractError> {
    if amount_out.is_zero() {
        return Ok(sqrt_price_x96);
    }
    let liquidity_u256 = as_u256(liquidity);
    if liquidity_u256.is_zero() {
        return Err(ContractError::new("zero liquidity"));
    }
    if zero_for_one {
        let delta = mul_div_rounding_up(amount_out, q96(), liquidity_u256)?;
        sqrt_price_x96.checked_sub(delta).map_err(ContractError::from)
    } else {
        let product = mul_div_rounding_up(amount_out, sqrt_price_x96, q96())?;
        let denominator = liquidity_u256.checked_sub(product)?;
        mul_div_rounding_up(liquidity_u256, sqrt_price_x96, denominator)
    }
}
