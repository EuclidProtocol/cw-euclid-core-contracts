use alloy_sol_types::sol_data::{String as SolString, Uint as SolUint};
use alloy_sol_types::SolType;
use euclid::token::TokenType;
use euclid_encoding::abi::option::{opt_prim_from_sol, OptPrimSol};
use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::{ensure_empty, tagged, TaggedSol};
use euclid_encoding::EncodingError;

const TAG_NATIVE: u8 = 0;
const TAG_SMART: u8 = 1;
const TAG_VOUCHER: u8 = 2;

pub type TokenTypeSol = TaggedSol;

type NativeOrSmartSol = (SolString, OptPrimSol<SolUint<32>>);

pub fn token_type_to_sol(
    v: &TokenType,
) -> Result<<TokenTypeSol as SolType>::RustType, EncodingError> {
    Ok(match v {
        TokenType::Native { denom, decimals } => tagged(
            TAG_NATIVE,
            <NativeOrSmartSol>::abi_encode_params(&(
                denom.clone(),
                (decimals.is_some(), decimals.unwrap_or_default()),
            )),
        ),
        TokenType::Smart {
            contract_address,
            decimals,
        } => tagged(
            TAG_SMART,
            <NativeOrSmartSol>::abi_encode_params(&(
                contract_address.clone(),
                (decimals.is_some(), decimals.unwrap_or_default()),
            )),
        ),
        TokenType::Voucher {} => tagged(TAG_VOUCHER, Vec::new()),
    })
}

pub fn token_type_from_sol(
    sol: <TokenTypeSol as SolType>::RustType,
) -> Result<TokenType, EncodingError> {
    let (tag, data) = sol;
    match tag {
        TAG_NATIVE => {
            let (denom, decimals) =
                decode_params_canonical::<NativeOrSmartSol>("TokenType", &data)?;
            Ok(TokenType::Native {
                denom,
                decimals: opt_prim_from_sol("TokenType", decimals)?,
            })
        }
        TAG_SMART => {
            let (contract_address, decimals) =
                decode_params_canonical::<NativeOrSmartSol>("TokenType", &data)?;
            Ok(TokenType::Smart {
                contract_address,
                decimals: opt_prim_from_sol("TokenType", decimals)?,
            })
        }
        TAG_VOUCHER => {
            ensure_empty("TokenType", &data)?;
            Ok(TokenType::Voucher {})
        }
        other => Err(EncodingError::UnknownDiscriminant {
            type_name: "TokenType",
            discriminant: other,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: TokenType) {
        let sol = token_type_to_sol(&v).unwrap();
        let back = token_type_from_sol(sol).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn token_type_sol_roundtrip_native_with_decimals() {
        roundtrip(TokenType::Native {
            denom: "uatom".to_string(),
            decimals: Some(6),
        });
    }

    #[test]
    fn token_type_sol_roundtrip_native_without_decimals() {
        roundtrip(TokenType::Native {
            denom: "uatom".to_string(),
            decimals: None,
        });
    }

    #[test]
    fn token_type_sol_roundtrip_smart() {
        roundtrip(TokenType::Smart {
            contract_address: "cosmos1contract".to_string(),
            decimals: Some(18),
        });
    }

    #[test]
    fn token_type_sol_roundtrip_voucher() {
        roundtrip(TokenType::Voucher {});
    }

    #[test]
    fn token_type_from_sol_rejects_unknown_tag() {
        assert!(token_type_from_sol(tagged(99, Vec::new())).is_err());
    }
}
