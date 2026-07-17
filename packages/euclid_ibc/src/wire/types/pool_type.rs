use alloy_sol_types::sol_data::Uint as SolUint;
use alloy_sol_types::SolType;
use euclid::msgs::vlp::base::PoolType;
use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::{ensure_empty, tagged, TaggedSol};
use euclid_encoding::EncodingError;

const TAG_CONSTANT_PRODUCT: u8 = 0;
const TAG_STABLE: u8 = 1;
const TAG_CONCENTRATED: u8 = 2;

pub type PoolTypeSol = TaggedSol;

type ConcentratedSol = (SolUint<64>, SolUint<64>);

pub fn pool_type_to_sol(v: &PoolType) -> Result<<PoolTypeSol as SolType>::RustType, EncodingError> {
    Ok(match v {
        PoolType::ConstantProduct {} => tagged(TAG_CONSTANT_PRODUCT, Vec::new()),
        PoolType::Stable {} => tagged(TAG_STABLE, Vec::new()),
        PoolType::Concentrated {
            fee_tier_bps,
            tick_spacing,
        } => tagged(
            TAG_CONCENTRATED,
            <ConcentratedSol>::abi_encode_params(&(*fee_tier_bps, *tick_spacing)),
        ),
    })
}

pub fn pool_type_from_sol(
    sol: <PoolTypeSol as SolType>::RustType,
) -> Result<PoolType, EncodingError> {
    let (tag, data) = sol;
    match tag {
        TAG_CONSTANT_PRODUCT => {
            ensure_empty("PoolType", &data)?;
            Ok(PoolType::ConstantProduct {})
        }
        TAG_STABLE => {
            ensure_empty("PoolType", &data)?;
            Ok(PoolType::Stable {})
        }
        TAG_CONCENTRATED => {
            let (fee_tier_bps, tick_spacing) =
                decode_params_canonical::<ConcentratedSol>("PoolType", &data)?;
            Ok(PoolType::Concentrated {
                fee_tier_bps,
                tick_spacing,
            })
        }
        other => Err(EncodingError::UnknownDiscriminant {
            type_name: "PoolType",
            discriminant: other,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: PoolType) {
        let sol = pool_type_to_sol(&v).unwrap();
        let back = pool_type_from_sol(sol).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn pool_type_sol_roundtrip_constant_product() {
        roundtrip(PoolType::ConstantProduct {});
    }

    #[test]
    fn pool_type_sol_roundtrip_stable() {
        roundtrip(PoolType::Stable {});
    }

    #[test]
    fn pool_type_sol_roundtrip_concentrated() {
        roundtrip(PoolType::Concentrated {
            fee_tier_bps: 30,
            tick_spacing: 60,
        });
    }

    #[test]
    fn pool_type_from_sol_rejects_unknown_tag() {
        assert!(pool_type_from_sol(tagged(99, Vec::new())).is_err());
    }
}
