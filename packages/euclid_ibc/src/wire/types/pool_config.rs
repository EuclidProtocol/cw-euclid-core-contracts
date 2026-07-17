use alloy_sol_types::sol_data::Uint as SolUint;
use alloy_sol_types::SolType;
use euclid::msgs::vlp::base::PoolConfig;
use euclid_encoding::abi::bridge::{uint64_from_sol, uint64_to_sol};
use euclid_encoding::abi::option::{opt_prim_from_sol, OptPrimSol};
use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::{ensure_empty, tagged, TaggedSol};
use euclid_encoding::EncodingError;

const TAG_STABLE: u8 = 0;
const TAG_CONSTANT_PRODUCT: u8 = 1;
const TAG_CONCENTRATED: u8 = 2;

pub type PoolConfigSol = TaggedSol;

type AmpFactorSol = OptPrimSol<SolUint<64>>;
type ConcentratedSol = (SolUint<64>, SolUint<64>);

pub fn pool_config_to_sol(
    v: &PoolConfig,
) -> Result<<PoolConfigSol as SolType>::RustType, EncodingError> {
    Ok(match v {
        PoolConfig::Stable { amp_factor } => tagged(
            TAG_STABLE,
            <AmpFactorSol>::abi_encode_params(&(
                amp_factor.is_some(),
                amp_factor.as_ref().map(uint64_to_sol).unwrap_or_default(),
            )),
        ),
        PoolConfig::ConstantProduct {} => tagged(TAG_CONSTANT_PRODUCT, Vec::new()),
        PoolConfig::Concentrated {
            fee_tier_bps,
            tick_spacing,
        } => tagged(
            TAG_CONCENTRATED,
            <ConcentratedSol>::abi_encode_params(&(*fee_tier_bps, *tick_spacing)),
        ),
    })
}

pub fn pool_config_from_sol(
    sol: <PoolConfigSol as SolType>::RustType,
) -> Result<PoolConfig, EncodingError> {
    let (tag, data) = sol;
    match tag {
        TAG_STABLE => {
            let amp = decode_params_canonical::<AmpFactorSol>("PoolConfig", &data)?;
            Ok(PoolConfig::Stable {
                amp_factor: opt_prim_from_sol("PoolConfig", amp)?.map(uint64_from_sol),
            })
        }
        TAG_CONSTANT_PRODUCT => {
            ensure_empty("PoolConfig", &data)?;
            Ok(PoolConfig::ConstantProduct {})
        }
        TAG_CONCENTRATED => {
            let (fee_tier_bps, tick_spacing) =
                decode_params_canonical::<ConcentratedSol>("PoolConfig", &data)?;
            Ok(PoolConfig::Concentrated {
                fee_tier_bps,
                tick_spacing,
            })
        }
        other => Err(EncodingError::UnknownDiscriminant {
            type_name: "PoolConfig",
            discriminant: other,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::Uint64;

    fn roundtrip(v: PoolConfig) {
        let sol = pool_config_to_sol(&v).unwrap();
        let back = pool_config_from_sol(sol).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn pool_config_sol_roundtrip_stable_with_amp() {
        roundtrip(PoolConfig::Stable {
            amp_factor: Some(Uint64::from(100u64)),
        });
    }

    #[test]
    fn pool_config_sol_roundtrip_stable_without_amp() {
        roundtrip(PoolConfig::Stable { amp_factor: None });
    }

    #[test]
    fn pool_config_sol_roundtrip_constant_product() {
        roundtrip(PoolConfig::ConstantProduct {});
    }

    #[test]
    fn pool_config_sol_roundtrip_concentrated() {
        roundtrip(PoolConfig::Concentrated {
            fee_tier_bps: 30,
            tick_spacing: 60,
        });
    }

    #[test]
    fn pool_config_from_sol_rejects_unknown_tag() {
        assert!(pool_config_from_sol(tagged(99, Vec::new())).is_err());
    }
}
