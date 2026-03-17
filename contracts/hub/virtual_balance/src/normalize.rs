use cosmwasm_std::Uint256;
use euclid::{error::ContractError, token::TokenMetadata};

use crate::state::VOUCHER_DECIMAL;

pub fn normalize_token_to_voucher(
    token_amount: Uint256,
    metadata: TokenMetadata,
) -> Result<Uint256, ContractError> {
    normalize(token_amount, metadata.decimals, VOUCHER_DECIMAL)
}

pub fn normalize_voucher_to_token(
    voucher_amount: Uint256,
    metadata: TokenMetadata,
) -> Result<Uint256, ContractError> {
    normalize(voucher_amount, VOUCHER_DECIMAL, metadata.decimals)
}

pub fn normalize(
    amount: Uint256,
    from_decimals: u8,
    to_decimals: u8,
) -> Result<Uint256, ContractError> {
    if from_decimals == to_decimals {
        return Ok(amount);
    }
    if to_decimals > from_decimals {
        let factor =
            Uint256::from(10u128).checked_pow((to_decimals - from_decimals) as u32)?;
        Ok(amount.checked_mul(factor)?)
    } else {
        let factor =
            Uint256::from(10u128).checked_pow((from_decimals - to_decimals) as u32)?;
        Ok(amount.checked_div(factor)?)
    }
}
