use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use euclid::token::TokenWithDenom;
use euclid_encoding::abi::bridge::newtype_from_string;
use euclid_encoding::EncodingError;

use super::token_type::{token_type_from_sol, token_type_to_sol, TokenTypeSol};

pub type TokenWithDenomSol = (SolString, TokenTypeSol);

pub fn token_with_denom_to_sol(
    v: &TokenWithDenom,
) -> Result<<TokenWithDenomSol as SolType>::RustType, EncodingError> {
    Ok((v.token.to_string(), token_type_to_sol(&v.token_type)?))
}

pub fn token_with_denom_from_sol(
    sol: <TokenWithDenomSol as SolType>::RustType,
) -> Result<TokenWithDenom, EncodingError> {
    let (token, token_type) = sol;
    Ok(TokenWithDenom {
        token: newtype_from_string("Token", token)?,
        token_type: token_type_from_sol(token_type)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::token::{Token, TokenType};

    #[test]
    fn token_with_denom_sol_roundtrip() {
        let value = TokenWithDenom {
            token: Token::create("usdt".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "uusdt".to_string(),
                decimals: Some(6),
            },
        };
        let sol = token_with_denom_to_sol(&value).unwrap();
        let back = token_with_denom_from_sol(sol).unwrap();
        assert_eq!(value, back);
    }
}
