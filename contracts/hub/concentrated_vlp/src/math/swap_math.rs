use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Uint128, Uint256};
use euclid::error::ContractError;

use crate::math::{
    full_math::{mul_div, mul_div_rounding_up},
    sqrt_price_math::{get_amount0_delta, get_amount1_delta, get_next_sqrt_price_from_input},
};

pub const FEE_DENOMINATOR_PIPS: u64 = 1_000_000;

#[cw_serde]
pub struct SwapStepResult {
    pub sqrt_ratio_next_x96: Uint256,
    pub amount_in: Uint256,
    pub amount_out: Uint256,
    pub fee_amount: Uint256,
}

pub fn compute_swap_step_exact_input(
    sqrt_ratio_current_x96: Uint256,
    sqrt_ratio_target_x96: Uint256,
    liquidity: Uint128,
    amount_remaining: Uint256,
    fee_pips: u64,
) -> Result<SwapStepResult, ContractError> {
    if liquidity.is_zero() {
        return Err(ContractError::new("zero liquidity"));
    }
    if fee_pips >= FEE_DENOMINATOR_PIPS {
        return Err(ContractError::new("invalid fee pips"));
    }

    let zero_for_one = sqrt_ratio_current_x96 >= sqrt_ratio_target_x96;
    let fee_pips_u256 = Uint256::from(u128::from(fee_pips));
    let fee_denom_u256 = Uint256::from(u128::from(FEE_DENOMINATOR_PIPS));

    let amount_remaining_less_fee = mul_div(
        amount_remaining,
        fee_denom_u256.checked_sub(fee_pips_u256)?,
        fee_denom_u256,
    )?;

    let amount_in_max = if zero_for_one {
        get_amount0_delta(
            sqrt_ratio_target_x96,
            sqrt_ratio_current_x96,
            liquidity,
            true,
        )?
    } else {
        get_amount1_delta(
            sqrt_ratio_current_x96,
            sqrt_ratio_target_x96,
            liquidity,
            true,
        )?
    };

    let sqrt_ratio_next_x96 = if amount_remaining_less_fee >= amount_in_max {
        sqrt_ratio_target_x96
    } else {
        get_next_sqrt_price_from_input(
            sqrt_ratio_current_x96,
            liquidity,
            amount_remaining_less_fee,
            zero_for_one,
        )?
    };

    let reached_target = sqrt_ratio_next_x96 == sqrt_ratio_target_x96;

    let amount_in = if zero_for_one {
        get_amount0_delta(sqrt_ratio_next_x96, sqrt_ratio_current_x96, liquidity, true)?
    } else {
        get_amount1_delta(sqrt_ratio_current_x96, sqrt_ratio_next_x96, liquidity, true)?
    };

    let amount_out = if zero_for_one {
        get_amount1_delta(
            sqrt_ratio_next_x96,
            sqrt_ratio_current_x96,
            liquidity,
            false,
        )?
    } else {
        get_amount0_delta(
            sqrt_ratio_current_x96,
            sqrt_ratio_next_x96,
            liquidity,
            false,
        )?
    };

    let fee_amount = if reached_target {
        mul_div_rounding_up(
            amount_in,
            fee_pips_u256,
            fee_denom_u256.checked_sub(fee_pips_u256)?,
        )?
    } else {
        amount_remaining.checked_sub(amount_in)?
    };

    Ok(SwapStepResult {
        sqrt_ratio_next_x96,
        amount_in,
        amount_out,
        fee_amount,
    })
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Uint128, Uint256};

    use crate::math::swap_math::compute_swap_step_exact_input;
    use crate::math::tick_math::get_sqrt_ratio_at_tick;

    #[test]
    fn compute_swap_step_exact_input_vectors() {
        let sqrt_current = get_sqrt_ratio_at_tick(0).unwrap();
        let sqrt_target = get_sqrt_ratio_at_tick(-60).unwrap();
        let step = compute_swap_step_exact_input(
            sqrt_current,
            sqrt_target,
            Uint128::new(10_000_000),
            Uint256::from(100_000u128),
            3_000,
        )
        .unwrap();
        assert!(step.amount_in > Uint256::zero());
        assert!(step.amount_out > Uint256::zero());
        assert!(step.fee_amount > Uint256::zero());
        assert!(step.sqrt_ratio_next_x96 <= sqrt_current);
    }

    #[test]
    fn compute_swap_step_exact_output_vectors() {
        // Exact-output path is not surfaced yet; this test guards zero/overflow regressions.
        let sqrt_current = get_sqrt_ratio_at_tick(0).unwrap();
        let sqrt_target = get_sqrt_ratio_at_tick(60).unwrap();
        let step = compute_swap_step_exact_input(
            sqrt_current,
            sqrt_target,
            Uint128::new(9_000_000),
            Uint256::from(250_000u128),
            500,
        )
        .unwrap();
        assert!(step.sqrt_ratio_next_x96 >= sqrt_current);
    }
}
