use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use euclid::token::TokenWithDenomAndAmount;
use euclid_encoding::abi::bridge::{newtype_from_string, uint256_from_sol, uint256_to_sol};
use euclid_encoding::EncodingError;

use super::token_type::{token_type_from_sol, token_type_to_sol, TokenTypeSol};

pub type TokenWithDenomAndAmountSol = (SolString, SolUint<256>, TokenTypeSol);

pub fn token_with_denom_and_amount_to_sol(
    v: &TokenWithDenomAndAmount,
) -> Result<<TokenWithDenomAndAmountSol as SolType>::RustType, EncodingError> {
    Ok((
        v.token.to_string(),
        uint256_to_sol(&v.amount),
        token_type_to_sol(&v.token_type)?,
    ))
}

pub fn token_with_denom_and_amount_from_sol(
    sol: <TokenWithDenomAndAmountSol as SolType>::RustType,
) -> Result<TokenWithDenomAndAmount, EncodingError> {
    let (token, amount, token_type) = sol;
    Ok(TokenWithDenomAndAmount {
        token: newtype_from_string("Token", token)?,
        amount: uint256_from_sol(amount),
        token_type: token_type_from_sol(token_type)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::token::{Token, TokenType};

    #[test]
    fn token_with_denom_and_amount_sol_roundtrip() {
        let value = TokenWithDenomAndAmount {
            token: Token::create("usdt".to_string()).unwrap(),
            amount: Uint256::from(9_999u128),
            token_type: TokenType::Smart {
                contract_address: "cosmos1contract".to_string(),
                decimals: Some(18),
            },
        };
        let sol = token_with_denom_and_amount_to_sol(&value).unwrap();
        let back = token_with_denom_and_amount_from_sol(sol).unwrap();
        assert_eq!(value, back);
    }
}
