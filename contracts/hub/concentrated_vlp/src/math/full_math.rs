use cosmwasm_std::{Uint256, Uint512};
use euclid::error::ContractError;

pub fn mul_div(a: Uint256, b: Uint256, denominator: Uint256) -> Result<Uint256, ContractError> {
    if denominator.is_zero() {
        return Err(ContractError::new("division by zero"));
    }
    let product = Uint512::from(a).checked_mul(Uint512::from(b))?;
    let quotient = product.checked_div(Uint512::from(denominator))?;
    Uint256::try_from(quotient).map_err(|_| ContractError::new("mul_div overflow"))
}

pub fn mul_div_rounding_up(
    a: Uint256,
    b: Uint256,
    denominator: Uint256,
) -> Result<Uint256, ContractError> {
    let result = mul_div(a, b, denominator)?;
    let rem = Uint512::from(a)
        .checked_mul(Uint512::from(b))?
        .checked_rem(Uint512::from(denominator))?;
    if rem.is_zero() {
        Ok(result)
    } else {
        result
            .checked_add(Uint256::one())
            .map_err(ContractError::from)
    }
}

pub fn div_rounding_up(numerator: Uint256, denominator: Uint256) -> Result<Uint256, ContractError> {
    if denominator.is_zero() {
        return Err(ContractError::new("division by zero"));
    }
    let quotient = numerator.checked_div(denominator)?;
    let rem = numerator.checked_rem(denominator)?;
    if rem.is_zero() {
        Ok(quotient)
    } else {
        quotient
            .checked_add(Uint256::one())
            .map_err(ContractError::from)
    }
}
