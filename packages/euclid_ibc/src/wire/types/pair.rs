use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use euclid::token::Pair;
use euclid_encoding::abi::bridge::newtype_from_string;
use euclid_encoding::EncodingError;

pub type PairSol = (SolString, SolString);

pub fn pair_to_sol(v: &Pair) -> Result<<PairSol as SolType>::RustType, EncodingError> {
    Ok((v.token_1.to_string(), v.token_2.to_string()))
}

pub fn pair_from_sol(sol: <PairSol as SolType>::RustType) -> Result<Pair, EncodingError> {
    let (token_1, token_2) = sol;
    Ok(Pair {
        token_1: newtype_from_string("Token", token_1)?,
        token_2: newtype_from_string("Token", token_2)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::token::Token;

    #[test]
    fn pair_sol_roundtrip() {
        let pair = Pair {
            token_1: Token::create("aaa".to_string()).unwrap(),
            token_2: Token::create("bbb".to_string()).unwrap(),
        };
        let sol = pair_to_sol(&pair).unwrap();
        let back = pair_from_sol(sol).unwrap();
        assert_eq!(pair, back);
    }
}
