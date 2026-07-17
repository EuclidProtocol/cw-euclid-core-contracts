use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use euclid::token::Token;
use euclid_encoding::abi::bridge::newtype_from_string;
use euclid_encoding::EncodingError;

pub type TokenSol = (SolString,);

pub fn token_to_sol(v: &Token) -> Result<<TokenSol as SolType>::RustType, EncodingError> {
    Ok((v.to_string(),))
}

pub fn token_from_sol(sol: <TokenSol as SolType>::RustType) -> Result<Token, EncodingError> {
    let (s,) = sol;
    newtype_from_string("Token", s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_sol_roundtrip() {
        let token = Token::create("usdt".to_string()).unwrap();
        let sol = token_to_sol(&token).unwrap();
        let back = token_from_sol(sol).unwrap();
        assert_eq!(token, back);
    }
}
