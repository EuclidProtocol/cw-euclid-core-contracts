use alloy_sol_types::sol_data::String as SolString;
use alloy_sol_types::SolType;
use cosmwasm_schema::cw_serde;
use euclid::chain::ChainUid;
use euclid::msgs::factory::msg::RegisterFactoryResponse;
use euclid::msgs::router::execute::RegisterFactoryChainType;

use crate::wire::types::register_factory_chain::{
    register_factory_chain_type_from_sol, register_factory_chain_type_to_sol,
    RegisterFactoryChainTypeSol,
};
use euclid_encoding::abi::bridge::newtype_from_string;
use euclid_encoding::{AbiMap, EncodingError};

#[cw_serde]
pub struct RegisterFactorySendMsg {
    pub chain_uid: ChainUid,
    pub chain_type: RegisterFactoryChainType,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct RegisterFactoryAckMsg {
    pub factory_address: String,
    pub chain_id: String,
}

impl AbiMap for RegisterFactorySendMsg {
    type Sol = (SolString, RegisterFactoryChainTypeSol, SolString);

    fn type_name() -> &'static str {
        "RegisterFactorySendMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((
            self.chain_uid.to_string(),
            register_factory_chain_type_to_sol(&self.chain_type)?,
            self.tx_id.clone(),
        ))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        let (chain_uid, chain_type, tx_id) = sol;
        Ok(Self {
            chain_uid: newtype_from_string("ChainUid", chain_uid)?,
            chain_type: register_factory_chain_type_from_sol(chain_type)?,
            tx_id,
        })
    }
}

impl From<RegisterFactoryResponse> for RegisterFactoryAckMsg {
    fn from(v: RegisterFactoryResponse) -> Self {
        Self {
            factory_address: v.factory_address,
            chain_id: v.chain_id,
        }
    }
}

impl From<RegisterFactoryAckMsg> for RegisterFactoryResponse {
    fn from(v: RegisterFactoryAckMsg) -> Self {
        Self {
            factory_address: v.factory_address,
            chain_id: v.chain_id,
        }
    }
}

impl AbiMap for RegisterFactoryAckMsg {
    type Sol = (SolString, SolString);

    fn type_name() -> &'static str {
        "RegisterFactoryAckMsg"
    }

    fn to_sol(&self) -> Result<<Self::Sol as SolType>::RustType, EncodingError> {
        Ok((self.factory_address.clone(), self.chain_id.clone()))
    }

    fn from_sol(sol: <Self::Sol as SolType>::RustType) -> Result<Self, EncodingError> {
        Ok(Self {
            factory_address: sol.0,
            chain_id: sol.1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use euclid::msgs::router::execute::RegisterFactoryChainEvm;
    use euclid_encoding::{decode, encode, Encoding};

    fn send() -> RegisterFactorySendMsg {
        RegisterFactorySendMsg {
            chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
            chain_type: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                factory_address: "0xfactory".to_string(),
                factory_chain_id: "1".to_string(),
            }),
            tx_id: "tx-register".to_string(),
        }
    }

    fn ack() -> RegisterFactoryAckMsg {
        RegisterFactoryAckMsg {
            factory_address: "cosmos1factory".to_string(),
            chain_id: "cosmoshub-4".to_string(),
        }
    }

    #[test]
    fn send_json_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RegisterFactorySendMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn send_abi_roundtrips() {
        let msg = send();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RegisterFactorySendMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_json_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Json).unwrap();
        let decoded: RegisterFactoryAckMsg = decode(&bytes, Encoding::Json).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_abi_roundtrips() {
        let msg = ack();
        let bytes = encode(&msg, Encoding::Abi).unwrap();
        let decoded: RegisterFactoryAckMsg = decode(&bytes, Encoding::Abi).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn ack_from_conversion_roundtrips() {
        let domain = RegisterFactoryResponse::from(ack());
        let wire = RegisterFactoryAckMsg::from(domain.clone());
        assert_eq!(RegisterFactoryResponse::from(wire), domain);
    }
}
