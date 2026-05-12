use std::ops::Deref;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, StdError, StdResult};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};

use crate::error::ContractError;

#[cw_serde]
#[derive(PartialOrd)]
pub struct ChainUid(String);

// Implement Deref to allow easy access to the inner type
impl Deref for ChainUid {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ChainUid {
    fn new(uid: String) -> Self {
        Self(uid)
    }
    pub fn create(uid: String) -> Result<Self, ContractError> {
        let chain_uid = Self::new(uid);
        chain_uid.validate().cloned()
    }
    pub fn validate(&self) -> Result<&Self, ContractError> {
        ensure!(
            !self.0.is_empty(),
            ContractError::new("Chain UID cannot be empty")
        );
        for c in self.0.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '.' {
                return Err(ContractError::new(
                    "Invalid UID format: must be lowercase, alphanumeric or '.'",
                ));
            }
        }
        Ok(self)
    }

    pub fn vsl_chain_uid() -> Result<Self, ContractError> {
        Self::create("vsl".to_string())
    }
}

impl PrimaryKey<'_> for ChainUid {
    type Prefix = ();
    type SubPrefix = ();

    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key<'_>> {
        vec![Key::Ref(self.0.as_bytes())]
    }
}

impl Prefixer<'_> for ChainUid {
    fn prefix(&self) -> Vec<Key<'_>> {
        vec![Key::Ref(self.0.as_bytes())]
    }
}

impl KeyDeserialize for ChainUid {
    type Output = Self;
    const KEY_ELEMS: u16 = 1;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        String::from_utf8(value)
            .map(Self::create)
            .map_err(|e| StdError::msg(format!("Invalid UTF-8 sequence: {e}")))?
            .map_err(|err| StdError::msg(err.to_string()))
    }
}

#[cw_serde]
pub struct Chain {
    pub chain_uid: ChainUid,
    pub factory_address: String,
    pub chain_type: ChainType,
}

#[cw_serde]
pub struct CosmosChain {
    pub chain_id: String,
}

#[cw_serde]
pub struct EvmChain {
    pub chain_id: String,
}

#[cw_serde]
pub enum ChainType {
    Cosmos(CosmosChain),
    Evm(EvmChain),
    Native {},
}

impl Chain {
    #[must_use]
    pub fn is_native(&self) -> bool {
        matches!(self.chain_type, ChainType::Native {})
    }

    #[must_use]
    pub fn is_evm(&self) -> bool {
        matches!(self.chain_type, ChainType::Evm(_))
    }

    #[must_use]
    pub fn is_cosmos(&self) -> bool {
        matches!(self.chain_type, ChainType::Cosmos(_))
    }

    pub fn cosmos_info(&self) -> Result<CosmosChain, ContractError> {
        match self.chain_type.clone() {
            ChainType::Cosmos(data) => Ok(data),
            _ => Err(ContractError::new("Not a cosmos chain")),
        }
    }

    #[must_use]
    pub fn get_chain_type_str(&self) -> String {
        match self.chain_type {
            ChainType::Cosmos(_) => "cosmos".to_string(),
            ChainType::Evm(_) => "evm".to_string(),
            ChainType::Native {} => "native".to_string(),
        }
    }
}
