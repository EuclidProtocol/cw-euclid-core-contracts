use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use euclid::chain::ChainUid;
use euclid_encoding::abi::bridge::newtype_from_string;
use euclid_encoding::EncodingError;

pub type ChainUidSol = (SolString,);

pub fn chain_uid_to_sol(v: &ChainUid) -> Result<<ChainUidSol as SolType>::RustType, EncodingError> {
    Ok((v.to_string(),))
}

pub fn chain_uid_from_sol(
    sol: <ChainUidSol as SolType>::RustType,
) -> Result<ChainUid, EncodingError> {
    let (s,) = sol;
    newtype_from_string("ChainUid", s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_uid_sol_roundtrip() {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sol = chain_uid_to_sol(&chain_uid).unwrap();
        let back = chain_uid_from_sol(sol).unwrap();
        assert_eq!(chain_uid, back);
    }
}
