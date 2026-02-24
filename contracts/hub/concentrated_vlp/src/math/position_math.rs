use cosmwasm_std::{Uint128, Uint256};
use euclid::error::ContractError;

use crate::{
    math::{full_math::mul_div, sqrt_price_math::q128},
    state::TickInfo,
};

pub fn fee_growth_inside(
    current_tick: i64,
    lower_tick: i64,
    upper_tick: i64,
    fee_growth_global_0_x128: Uint256,
    fee_growth_global_1_x128: Uint256,
    lower: Option<TickInfo>,
    upper: Option<TickInfo>,
) -> Result<(Uint256, Uint256), ContractError> {
    let lower = lower.unwrap_or(TickInfo {
        initialized: false,
        liquidity_gross: Uint128::zero(),
        liquidity_net: 0,
        fee_growth_outside_0_x128: Uint256::zero(),
        fee_growth_outside_1_x128: Uint256::zero(),
    });
    let upper = upper.unwrap_or(TickInfo {
        initialized: false,
        liquidity_gross: Uint128::zero(),
        liquidity_net: 0,
        fee_growth_outside_0_x128: Uint256::zero(),
        fee_growth_outside_1_x128: Uint256::zero(),
    });

    let fee_growth_below_0 = if current_tick >= lower_tick {
        lower.fee_growth_outside_0_x128
    } else {
        fee_growth_global_0_x128.checked_sub(lower.fee_growth_outside_0_x128)?
    };
    let fee_growth_below_1 = if current_tick >= lower_tick {
        lower.fee_growth_outside_1_x128
    } else {
        fee_growth_global_1_x128.checked_sub(lower.fee_growth_outside_1_x128)?
    };

    let fee_growth_above_0 = if current_tick < upper_tick {
        upper.fee_growth_outside_0_x128
    } else {
        fee_growth_global_0_x128.checked_sub(upper.fee_growth_outside_0_x128)?
    };
    let fee_growth_above_1 = if current_tick < upper_tick {
        upper.fee_growth_outside_1_x128
    } else {
        fee_growth_global_1_x128.checked_sub(upper.fee_growth_outside_1_x128)?
    };

    let inside_0 = fee_growth_global_0_x128
        .checked_sub(fee_growth_below_0)?
        .checked_sub(fee_growth_above_0)?;
    let inside_1 = fee_growth_global_1_x128
        .checked_sub(fee_growth_below_1)?
        .checked_sub(fee_growth_above_1)?;

    Ok((inside_0, inside_1))
}

pub fn fees_owed(
    liquidity: Uint128,
    fee_growth_inside_x128: Uint256,
    fee_growth_inside_last_x128: Uint256,
) -> Result<Uint128, ContractError> {
    if fee_growth_inside_x128 <= fee_growth_inside_last_x128 || liquidity.is_zero() {
        return Ok(Uint128::zero());
    }
    let delta = fee_growth_inside_x128.checked_sub(fee_growth_inside_last_x128)?;
    let amount = mul_div(Uint256::from(liquidity.u128()), delta, q128())?;
    Uint128::try_from(amount).map_err(|_| ContractError::new("fees owed overflow"))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Uint128, Uint256};

    use crate::math::position_math::{fee_growth_inside, fees_owed};
    use crate::state::TickInfo;

    #[test]
    fn fee_growth_inside_calculation_vectors() {
        let lower = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: 1,
            fee_growth_outside_0_x128: Uint256::from(10u128),
            fee_growth_outside_1_x128: Uint256::from(20u128),
        };
        let upper = TickInfo {
            initialized: true,
            liquidity_gross: Uint128::new(1),
            liquidity_net: -1,
            fee_growth_outside_0_x128: Uint256::from(30u128),
            fee_growth_outside_1_x128: Uint256::from(40u128),
        };
        let (inside_0, inside_1) = fee_growth_inside(
            0,
            -10,
            10,
            Uint256::from(100u128),
            Uint256::from(200u128),
            Some(lower),
            Some(upper),
        )
        .unwrap();
        assert_eq!(inside_0, Uint256::from(60u128));
        assert_eq!(inside_1, Uint256::from(140u128));
        assert_eq!(
            fees_owed(Uint128::new(10_000), inside_0, Uint256::from(0u128)).unwrap(),
            Uint128::zero()
        );
    }
}
