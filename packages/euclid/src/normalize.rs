use crate::{error::ContractError, voucher::VOUCHER_DECIMAL};
use cosmwasm_std::Uint256;

pub fn normalize_token_to_voucher(
    token_amount: Uint256,
    decimals: u32,
) -> Result<Uint256, ContractError> {
    normalize(token_amount, decimals, VOUCHER_DECIMAL)
}

pub fn normalize_voucher_to_token(
    voucher_amount: Uint256,
    decimals: u32,
) -> Result<Uint256, ContractError> {
    normalize(voucher_amount, VOUCHER_DECIMAL, decimals)
}

pub fn normalize(
    amount: Uint256,
    from_decimals: u32,
    to_decimals: u32,
) -> Result<Uint256, ContractError> {
    if from_decimals == to_decimals {
        return Ok(amount);
    }
    if to_decimals > from_decimals {
        let factor = Uint256::from(10u128).checked_pow(to_decimals - from_decimals)?;
        Ok(amount.checked_mul(factor)?)
    } else {
        let factor = Uint256::from(10u128).checked_pow(from_decimals - to_decimals)?;
        Ok(amount.checked_div(factor)?)
    }
}
