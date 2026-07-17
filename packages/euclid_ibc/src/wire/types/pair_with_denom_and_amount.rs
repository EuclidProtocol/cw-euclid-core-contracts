use alloy_sol_types::SolType;
use euclid::token::PairWithDenomAndAmount;
use euclid_encoding::EncodingError;

use super::token_with_denom_and_amount::{
    token_with_denom_and_amount_from_sol, token_with_denom_and_amount_to_sol,
    TokenWithDenomAndAmountSol,
};

pub type PairWithDenomAndAmountSol = (TokenWithDenomAndAmountSol, TokenWithDenomAndAmountSol);

pub fn pair_with_denom_and_amount_to_sol(
    v: &PairWithDenomAndAmount,
) -> Result<<PairWithDenomAndAmountSol as SolType>::RustType, EncodingError> {
    Ok((
        token_with_denom_and_amount_to_sol(&v.token_1)?,
        token_with_denom_and_amount_to_sol(&v.token_2)?,
    ))
}

pub fn pair_with_denom_and_amount_from_sol(
    sol: <PairWithDenomAndAmountSol as SolType>::RustType,
) -> Result<PairWithDenomAndAmount, EncodingError> {
    let (token_1, token_2) = sol;
    Ok(PairWithDenomAndAmount {
        token_1: token_with_denom_and_amount_from_sol(token_1)?,
        token_2: token_with_denom_and_amount_from_sol(token_2)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::token::{Token, TokenType, TokenWithDenomAndAmount};

    #[test]
    fn pair_with_denom_and_amount_sol_roundtrip() {
        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                amount: Uint256::from(1_000u128),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                amount: Uint256::from(2_000u128),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: Some(6),
                },
            },
        };
        let sol = pair_with_denom_and_amount_to_sol(&pair).unwrap();
        let back = pair_with_denom_and_amount_from_sol(sol).unwrap();
        assert_eq!(pair, back);
    }
}
