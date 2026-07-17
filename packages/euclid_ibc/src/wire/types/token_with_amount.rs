use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use euclid::token::TokenWithAmount;
use euclid_encoding::abi::bridge::{newtype_from_string, uint256_from_sol, uint256_to_sol};
use euclid_encoding::EncodingError;

pub type TokenWithAmountSol = (SolString, SolUint<256>);

pub fn token_with_amount_to_sol(
    v: &TokenWithAmount,
) -> Result<<TokenWithAmountSol as SolType>::RustType, EncodingError> {
    Ok((v.token.to_string(), uint256_to_sol(&v.amount)))
}

pub fn token_with_amount_from_sol(
    sol: <TokenWithAmountSol as SolType>::RustType,
) -> Result<TokenWithAmount, EncodingError> {
    let (token, amount) = sol;
    Ok(TokenWithAmount {
        token: newtype_from_string("Token", token)?,
        amount: uint256_from_sol(amount),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint256;
    use euclid::token::Token;

    #[test]
    fn token_with_amount_sol_roundtrip() {
        let value = TokenWithAmount {
            token: Token::create("usdt".to_string()).unwrap(),
            amount: Uint256::from(1_234u128),
        };
        let sol = token_with_amount_to_sol(&value).unwrap();
        let back = token_with_amount_from_sol(sol).unwrap();
        assert_eq!(value, back);
    }
}
