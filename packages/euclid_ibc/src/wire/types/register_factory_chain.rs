use alloy_sol_types::private::Bytes;
use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use euclid::msgs::router::execute::{
    RegisterFactoryChainCosmos, RegisterFactoryChainEvm, RegisterFactoryChainNative,
    RegisterFactoryChainTvm, RegisterFactoryChainType,
};
use euclid_encoding::abi::params::decode_params_canonical;
use euclid_encoding::abi::tagged::{tagged, TaggedSol};
use euclid_encoding::EncodingError;

const TAG_NATIVE: u8 = 0;
const TAG_COSMOS: u8 = 1;
const TAG_EVM: u8 = 2;
const TAG_TVM: u8 = 3;

pub type RegisterFactoryChainTypeSol = TaggedSol;

type ChainInfoSol = (SolString, SolString);

fn encode_chain_info(factory_address: &str, factory_chain_id: &str) -> Vec<u8> {
    <ChainInfoSol>::abi_encode_params(&(factory_address.to_string(), factory_chain_id.to_string()))
}

fn decode_chain_info(data: &Bytes) -> Result<(String, String), EncodingError> {
    decode_params_canonical::<ChainInfoSol>("RegisterFactoryChainType", data)
}

pub fn register_factory_chain_type_to_sol(
    v: &RegisterFactoryChainType,
) -> Result<<RegisterFactoryChainTypeSol as SolType>::RustType, EncodingError> {
    Ok(match v {
        RegisterFactoryChainType::Native(info) => tagged(
            TAG_NATIVE,
            encode_chain_info(&info.factory_address, &info.factory_chain_id),
        ),
        RegisterFactoryChainType::Cosmos(info) => tagged(
            TAG_COSMOS,
            encode_chain_info(&info.factory_address, &info.factory_chain_id),
        ),
        RegisterFactoryChainType::Evm(info) => tagged(
            TAG_EVM,
            encode_chain_info(&info.factory_address, &info.factory_chain_id),
        ),
        RegisterFactoryChainType::Tvm(info) => tagged(
            TAG_TVM,
            encode_chain_info(&info.factory_address, &info.factory_chain_id),
        ),
    })
}

pub fn register_factory_chain_type_from_sol(
    sol: <RegisterFactoryChainTypeSol as SolType>::RustType,
) -> Result<RegisterFactoryChainType, EncodingError> {
    let (tag, data) = sol;
    match tag {
        TAG_NATIVE => {
            let (factory_address, factory_chain_id) = decode_chain_info(&data)?;
            Ok(RegisterFactoryChainType::Native(
                RegisterFactoryChainNative {
                    factory_address,
                    factory_chain_id,
                },
            ))
        }
        TAG_COSMOS => {
            let (factory_address, factory_chain_id) = decode_chain_info(&data)?;
            Ok(RegisterFactoryChainType::Cosmos(
                RegisterFactoryChainCosmos {
                    factory_address,
                    factory_chain_id,
                },
            ))
        }
        TAG_EVM => {
            let (factory_address, factory_chain_id) = decode_chain_info(&data)?;
            Ok(RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                factory_address,
                factory_chain_id,
            }))
        }
        TAG_TVM => {
            let (factory_address, factory_chain_id) = decode_chain_info(&data)?;
            Ok(RegisterFactoryChainType::Tvm(RegisterFactoryChainTvm {
                factory_address,
                factory_chain_id,
            }))
        }
        other => Err(EncodingError::UnknownDiscriminant {
            type_name: "RegisterFactoryChainType",
            discriminant: other,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: RegisterFactoryChainType) {
        let sol = register_factory_chain_type_to_sol(&v).unwrap();
        let back = register_factory_chain_type_from_sol(sol).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn register_factory_chain_type_sol_roundtrip_native() {
        roundtrip(RegisterFactoryChainType::Native(
            RegisterFactoryChainNative {
                factory_address: "addr-native".to_string(),
                factory_chain_id: "chain-native".to_string(),
            },
        ));
    }

    #[test]
    fn register_factory_chain_type_sol_roundtrip_cosmos() {
        roundtrip(RegisterFactoryChainType::Cosmos(
            RegisterFactoryChainCosmos {
                factory_address: "addr-cosmos".to_string(),
                factory_chain_id: "chain-cosmos".to_string(),
            },
        ));
    }

    #[test]
    fn register_factory_chain_type_sol_roundtrip_evm() {
        roundtrip(RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
            factory_address: "addr-evm".to_string(),
            factory_chain_id: "chain-evm".to_string(),
        }));
    }

    #[test]
    fn register_factory_chain_type_sol_roundtrip_tvm() {
        roundtrip(RegisterFactoryChainType::Tvm(RegisterFactoryChainTvm {
            factory_address: "addr-tvm".to_string(),
            factory_chain_id: "chain-tvm".to_string(),
        }));
    }

    #[test]
    fn register_factory_chain_type_from_sol_rejects_unknown_tag() {
        assert!(register_factory_chain_type_from_sol(tagged(99, Vec::new())).is_err());
    }
}
