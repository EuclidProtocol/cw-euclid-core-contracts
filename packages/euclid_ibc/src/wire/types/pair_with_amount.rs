use alloy_sol_types::SolType;
use euclid::token::PairWithAmount;
use euclid_encoding::EncodingError;

use super::token_with_amount::{
    token_with_amount_from_sol, token_with_amount_to_sol, TokenWithAmountSol,
};

pub type PairWithAmountSol = (TokenWithAmountSol, TokenWithAmountSol);

pub fn pair_with_amount_to_sol(
    v: &PairWithAmount,
) -> Result<<PairWithAmountSol as SolType>::RustType, EncodingError> {
    Ok((
        token_with_amount_to_sol(&v.token_1)?,
        token_with_amount_to_sol(&v.token_2)?,
    ))
}

pub fn pair_with_amount_from_sol(
    sol: <PairWithAmountSol as SolType>::RustType,
) -> Result<PairWithAmount, EncodingError> {
    let (token_1, token_2) = sol;
    Ok(PairWithAmount {
        token_1: token_with_amount_from_sol(token_1)?,
        token_2: token_with_amount_from_sol(token_2)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::token::{Token, TokenWithAmount};

    #[test]
    fn pair_with_amount_sol_roundtrip() {
        let pair = PairWithAmount {
            token_1: TokenWithAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                amount: Uint256::from(1_000u128),
            },
            token_2: TokenWithAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                amount: Uint256::from(2_000u128),
            },
        };
        let sol = pair_with_amount_to_sol(&pair).unwrap();
        let back = pair_with_amount_from_sol(sol).unwrap();
        assert_eq!(pair, back);
    }
}
