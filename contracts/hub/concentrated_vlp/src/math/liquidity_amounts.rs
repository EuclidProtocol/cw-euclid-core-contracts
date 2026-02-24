use cosmwasm_std::{Uint128, Uint256};
use euclid::error::ContractError;

use crate::math::{
    full_math::{div_rounding_up, mul_div, mul_div_rounding_up},
    sqrt_price_math::q96,
};

fn ordered_ratios(sqrt_a_x96: Uint256, sqrt_b_x96: Uint256) -> (Uint256, Uint256) {
    if sqrt_a_x96 <= sqrt_b_x96 {
        (sqrt_a_x96, sqrt_b_x96)
    } else {
        (sqrt_b_x96, sqrt_a_x96)
    }
}

fn u256_to_u128(value: Uint256) -> Result<Uint128, ContractError> {
    Uint128::try_from(value).map_err(|_| ContractError::new("uint128 overflow"))
}

pub fn get_liquidity_for_amount0(
    sqrt_a_x96: Uint256,
    sqrt_b_x96: Uint256,
    amount0: Uint128,
) -> Result<Uint128, ContractError> {
    let (sqrt_a_x96, sqrt_b_x96) = ordered_ratios(sqrt_a_x96, sqrt_b_x96);
    if amount0.is_zero() || sqrt_a_x96 == sqrt_b_x96 {
        return Ok(Uint128::zero());
    }
    let intermediate = mul_div(sqrt_a_x96, sqrt_b_x96, q96())?;
    let liquidity = mul_div(
        Uint256::from(amount0.u128()),
        intermediate,
        sqrt_b_x96.checked_sub(sqrt_a_x96)?,
    )?;
    u256_to_u128(liquidity)
}

pub fn get_liquidity_for_amount1(
    sqrt_a_x96: Uint256,
    sqrt_b_x96: Uint256,
    amount1: Uint128,
) -> Result<Uint128, ContractError> {
    let (sqrt_a_x96, sqrt_b_x96) = ordered_ratios(sqrt_a_x96, sqrt_b_x96);
    if amount1.is_zero() || sqrt_a_x96 == sqrt_b_x96 {
        return Ok(Uint128::zero());
    }
    let liquidity = mul_div(
        Uint256::from(amount1.u128()),
        q96(),
        sqrt_b_x96.checked_sub(sqrt_a_x96)?,
    )?;
    u256_to_u128(liquidity)
}

pub fn get_liquidity_for_amounts(
    sqrt_p_x96: Uint256,
    sqrt_a_x96: Uint256,
    sqrt_b_x96: Uint256,
    amount0: Uint128,
    amount1: Uint128,
) -> Result<Uint128, ContractError> {
    let (sqrt_a_x96, sqrt_b_x96) = ordered_ratios(sqrt_a_x96, sqrt_b_x96);
    if sqrt_a_x96 == sqrt_b_x96 {
        return Err(ContractError::new("invalid sqrt ratio range"));
    }

    if sqrt_p_x96 <= sqrt_a_x96 {
        get_liquidity_for_amount0(sqrt_a_x96, sqrt_b_x96, amount0)
    } else if sqrt_p_x96 < sqrt_b_x96 {
        let liquidity0 = get_liquidity_for_amount0(sqrt_p_x96, sqrt_b_x96, amount0)?;
        let liquidity1 = get_liquidity_for_amount1(sqrt_a_x96, sqrt_p_x96, amount1)?;
        Ok(liquidity0.min(liquidity1))
    } else {
        get_liquidity_for_amount1(sqrt_a_x96, sqrt_b_x96, amount1)
    }
}

pub fn get_amount0_for_liquidity(
    sqrt_a_x96: Uint256,
    sqrt_b_x96: Uint256,
    liquidity: Uint128,
    round_up: bool,
) -> Result<Uint256, ContractError> {
    let (sqrt_a_x96, sqrt_b_x96) = ordered_ratios(sqrt_a_x96, sqrt_b_x96);
    if liquidity.is_zero() || sqrt_a_x96 == sqrt_b_x96 {
        return Ok(Uint256::zero());
    }

    let liquidity_x96 = Uint256::from(liquidity.u128()) << 96u32;
    let numerator = sqrt_b_x96.checked_sub(sqrt_a_x96)?;

    if round_up {
        let intermediate = mul_div_rounding_up(liquidity_x96, numerator, sqrt_b_x96)?;
        div_rounding_up(intermediate, sqrt_a_x96)
    } else {
        let intermediate = mul_div(liquidity_x96, numerator, sqrt_b_x96)?;
        intermediate.checked_div(sqrt_a_x96).map_err(ContractError::from)
    }
}

pub fn get_amount1_for_liquidity(
    sqrt_a_x96: Uint256,
    sqrt_b_x96: Uint256,
    liquidity: Uint128,
    round_up: bool,
) -> Result<Uint256, ContractError> {
    let (sqrt_a_x96, sqrt_b_x96) = ordered_ratios(sqrt_a_x96, sqrt_b_x96);
    if liquidity.is_zero() || sqrt_a_x96 == sqrt_b_x96 {
        return Ok(Uint256::zero());
    }
    let numerator = sqrt_b_x96.checked_sub(sqrt_a_x96)?;
    if round_up {
        mul_div_rounding_up(Uint256::from(liquidity.u128()), numerator, q96())
    } else {
        mul_div(Uint256::from(liquidity.u128()), numerator, q96())
    }
}

pub fn get_amounts_for_liquidity(
    sqrt_p_x96: Uint256,
    sqrt_a_x96: Uint256,
    sqrt_b_x96: Uint256,
    liquidity: Uint128,
    round_up: bool,
) -> Result<(Uint256, Uint256), ContractError> {
    let (sqrt_a_x96, sqrt_b_x96) = ordered_ratios(sqrt_a_x96, sqrt_b_x96);
    if sqrt_a_x96 == sqrt_b_x96 {
        return Err(ContractError::new("invalid sqrt ratio range"));
    }

    if sqrt_p_x96 <= sqrt_a_x96 {
        Ok((
            get_amount0_for_liquidity(sqrt_a_x96, sqrt_b_x96, liquidity, round_up)?,
            Uint256::zero(),
        ))
    } else if sqrt_p_x96 < sqrt_b_x96 {
        Ok((
            get_amount0_for_liquidity(sqrt_p_x96, sqrt_b_x96, liquidity, round_up)?,
            get_amount1_for_liquidity(sqrt_a_x96, sqrt_p_x96, liquidity, round_up)?,
        ))
    } else {
        Ok((
            Uint256::zero(),
            get_amount1_for_liquidity(sqrt_a_x96, sqrt_b_x96, liquidity, round_up)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Uint128, Uint256};

    use crate::math::{
        liquidity_amounts::{
            get_amounts_for_liquidity, get_liquidity_for_amount0, get_liquidity_for_amount1,
            get_liquidity_for_amounts,
        },
        sqrt_price_math::q96,
    };

    fn q96_mul(numerator: u128, denominator: u128) -> Uint256 {
        q96()
            .checked_mul(Uint256::from(numerator))
            .unwrap()
            .checked_div(Uint256::from(denominator))
            .unwrap()
    }

    #[test]
    fn below_range_uses_token0_only() {
        let sqrt_a = q96();
        let sqrt_b = q96_mul(2, 1);
        let sqrt_p = q96_mul(1, 2);
        let liquidity = get_liquidity_for_amounts(
            sqrt_p,
            sqrt_a,
            sqrt_b,
            Uint128::new(100),
            Uint128::new(100),
        )
        .unwrap();
        assert_eq!(liquidity, Uint128::new(200));

        let amounts = get_amounts_for_liquidity(sqrt_p, sqrt_a, sqrt_b, liquidity, false).unwrap();
        assert_eq!(amounts.0, Uint256::from(100u128));
        assert_eq!(amounts.1, Uint256::zero());
    }

    #[test]
    fn above_range_uses_token1_only() {
        let sqrt_a = q96();
        let sqrt_b = q96_mul(2, 1);
        let sqrt_p = q96_mul(3, 1);
        let liquidity = get_liquidity_for_amounts(
            sqrt_p,
            sqrt_a,
            sqrt_b,
            Uint128::new(100),
            Uint128::new(100),
        )
        .unwrap();
        assert_eq!(liquidity, Uint128::new(100));

        let amounts = get_amounts_for_liquidity(sqrt_p, sqrt_a, sqrt_b, liquidity, false).unwrap();
        assert_eq!(amounts.0, Uint256::zero());
        assert_eq!(amounts.1, Uint256::from(100u128));
    }

    #[test]
    fn in_range_is_limited_by_min_side() {
        let sqrt_a = q96();
        let sqrt_b = q96_mul(2, 1);
        let sqrt_p = q96_mul(3, 2);
        let liquidity = get_liquidity_for_amounts(
            sqrt_p,
            sqrt_a,
            sqrt_b,
            Uint128::new(100),
            Uint128::new(100),
        )
        .unwrap();
        assert_eq!(liquidity, Uint128::new(200));

        let (amount0_down, amount1_down) =
            get_amounts_for_liquidity(sqrt_p, sqrt_a, sqrt_b, liquidity, false).unwrap();
        assert_eq!(amount0_down, Uint256::from(33u128));
        assert_eq!(amount1_down, Uint256::from(100u128));
    }

    #[test]
    fn amount_liquidity_roundtrip_vectors() {
        let sqrt_a = q96();
        let sqrt_b = q96_mul(2, 1);

        let liquidity0 =
            get_liquidity_for_amount0(sqrt_a, sqrt_b, Uint128::new(100)).unwrap();
        let amount0 = get_amounts_for_liquidity(
            q96_mul(1, 2),
            sqrt_a,
            sqrt_b,
            liquidity0,
            false,
        )
        .unwrap()
        .0;
        assert_eq!(amount0, Uint256::from(100u128));

        let liquidity1 =
            get_liquidity_for_amount1(sqrt_a, sqrt_b, Uint128::new(100)).unwrap();
        let amount1 = get_amounts_for_liquidity(
            q96_mul(3, 1),
            sqrt_a,
            sqrt_b,
            liquidity1,
            false,
        )
        .unwrap()
        .1;
        assert_eq!(amount1, Uint256::from(100u128));
    }

    #[test]
    fn rounded_up_amounts_can_be_bounded_by_reducing_liquidity() {
        let sqrt_a = q96();
        let sqrt_b = q96_mul(2, 1);
        let sqrt_p = q96_mul(3, 2);
        let provided0 = Uint128::new(100);
        let provided1 = Uint128::new(100);
        let mut liquidity =
            get_liquidity_for_amounts(sqrt_p, sqrt_a, sqrt_b, provided0, provided1).unwrap();

        for _ in 0..8 {
            let (used0, used1) =
                get_amounts_for_liquidity(sqrt_p, sqrt_a, sqrt_b, liquidity, true).unwrap();
            let used0 = Uint128::try_from(used0).unwrap();
            let used1 = Uint128::try_from(used1).unwrap();
            if used0 <= provided0 && used1 <= provided1 {
                return;
            }
            liquidity = liquidity.checked_sub(Uint128::new(1)).unwrap();
        }
        panic!("failed to find a bounded rounded-up liquidity amount");
    }
}
