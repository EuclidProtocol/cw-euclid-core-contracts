use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::Uint as SolUint;
use alloy_sol_types::SolType;
use cosmwasm_std::Uint256;
use euclid::limit::Limit;
use euclid_encoding::abi::bridge::{uint256_from_sol, uint256_to_sol};
use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::{tagged, TaggedSol};
use euclid_encoding::EncodingError;

const TAG_LESS_THAN_OR_EQUAL: u8 = 0;
const TAG_EQUAL: u8 = 1;
const TAG_GREATER_THAN_OR_EQUAL: u8 = 2;
const TAG_DYNAMIC: u8 = 3;

pub type LimitSol = TaggedSol;

type AmountSol = (SolUint<256>,);

fn encode_amount(amount: &Uint256) -> Vec<u8> {
    <AmountSol>::abi_encode_params(&(uint256_to_sol(amount),))
}

fn decode_amount(data: &Bytes) -> Result<Uint256, EncodingError> {
    let (amount,) = decode_params_canonical::<AmountSol>("Limit", data)?;
    Ok(uint256_from_sol(amount))
}

pub fn limit_to_sol(v: &Limit) -> Result<<LimitSol as SolType>::RustType, EncodingError> {
    Ok(match v {
        Limit::LessThanOrEqual(amount) => tagged(TAG_LESS_THAN_OR_EQUAL, encode_amount(amount)),
        Limit::Equal(amount) => tagged(TAG_EQUAL, encode_amount(amount)),
        Limit::GreaterThanOrEqual(amount) => {
            tagged(TAG_GREATER_THAN_OR_EQUAL, encode_amount(amount))
        }
        Limit::Dynamic(amount) => tagged(TAG_DYNAMIC, encode_amount(amount)),
    })
}

pub fn limit_from_sol(sol: <LimitSol as SolType>::RustType) -> Result<Limit, EncodingError> {
    let (tag, data) = sol;
    match tag {
        TAG_LESS_THAN_OR_EQUAL => Ok(Limit::LessThanOrEqual(decode_amount(&data)?)),
        TAG_EQUAL => Ok(Limit::Equal(decode_amount(&data)?)),
        TAG_GREATER_THAN_OR_EQUAL => Ok(Limit::GreaterThanOrEqual(decode_amount(&data)?)),
        TAG_DYNAMIC => Ok(Limit::Dynamic(decode_amount(&data)?)),
        other => Err(EncodingError::UnknownDiscriminant {
            type_name: "Limit",
            discriminant: other,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(limit: Limit) {
        let sol = limit_to_sol(&limit).unwrap();
        let back = limit_from_sol(sol).unwrap();
        assert_eq!(limit, back);
    }

    #[test]
    fn limit_sol_roundtrip_less_than_or_equal() {
        roundtrip(Limit::LessThanOrEqual(Uint256::from(1u128)));
    }

    #[test]
    fn limit_sol_roundtrip_equal() {
        roundtrip(Limit::Equal(Uint256::from(2u128)));
    }

    #[test]
    fn limit_sol_roundtrip_greater_than_or_equal() {
        roundtrip(Limit::GreaterThanOrEqual(Uint256::from(3u128)));
    }

    #[test]
    fn limit_sol_roundtrip_dynamic() {
        roundtrip(Limit::Dynamic(Uint256::MAX));
    }

    #[test]
    fn limit_from_sol_rejects_unknown_tag() {
        assert!(limit_from_sol(tagged(99, Vec::new())).is_err());
    }
}
