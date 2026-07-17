use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use euclid::cross_chain_user::CrossChainUser;
use euclid_encoding::abi::bridge::newtype_from_string;
use euclid_encoding::EncodingError;

pub type CrossChainUserSol = (SolString, SolString);

pub fn cross_chain_user_to_sol(
    v: &CrossChainUser,
) -> Result<<CrossChainUserSol as SolType>::RustType, EncodingError> {
    Ok((v.chain_uid.to_string(), v.address.clone()))
}

pub fn cross_chain_user_from_sol(
    sol: <CrossChainUserSol as SolType>::RustType,
) -> Result<CrossChainUser, EncodingError> {
    let (chain_uid, address) = sol;
    Ok(CrossChainUser {
        chain_uid: newtype_from_string("ChainUid", chain_uid)?,
        address,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::chain::ChainUid;

    #[test]
    fn cross_chain_user_sol_roundtrip() {
        let user = CrossChainUser::new(
            ChainUid::create("chain1".to_string()).unwrap(),
            "addr1".to_string(),
        );
        let sol = cross_chain_user_to_sol(&user).unwrap();
        let back = cross_chain_user_from_sol(sol).unwrap();
        assert_eq!(user, back);
    }
}
