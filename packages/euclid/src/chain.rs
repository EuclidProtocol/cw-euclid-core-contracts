use std::ops::Deref;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, StdError, StdResult, Uint128};
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

impl<'a> PrimaryKey<'a> for ChainUid {
    type Prefix = ();
    type SubPrefix = ();

    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.0.as_bytes())]
    }
}

impl<'a> Prefixer<'a> for ChainUid {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.0.as_bytes())]
    }
}

impl KeyDeserialize for ChainUid {
    type Output = Self;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        String::from_utf8(value)
            .map(Self::create)
            .map_err(|e| StdError::generic_err(format!("Invalid UTF-8 sequence: {}", e)))?
            .map_err(|err| StdError::generic_err(err.to_string()))
    }
}

#[cw_serde]
pub struct CrossChainUser {
    pub chain_uid: ChainUid,
    pub address: String,
}

impl CrossChainUser {
    pub fn to_sender_string(&self) -> String {
        format!(
            "{chain}:{address}",
            chain = self.chain_uid.as_str(),
            address = self.address.as_str()
        )
    }
}

#[cw_serde]
pub struct CrossChainUserWithLimit {
    pub user: CrossChainUser,
    pub limit: Option<Uint128>,
}

#[cw_serde]
pub struct Chain {
    pub factory_chain_id: String,
    pub factory: String,
    pub chain_type: ChainType,
}

#[cw_serde]
pub struct IbcChain {
    pub from_hub_channel: String,
    pub from_factory_channel: String,
}

#[cw_serde]
pub enum ChainType {
    Ibc(IbcChain),
    Native {},
}

impl Chain {
    pub fn is_native(&self) -> bool {
        matches!(self.chain_type, ChainType::Native {})
    }

    pub fn ibc_info(&self) -> Result<IbcChain, ContractError> {
        match self.chain_type.clone() {
            ChainType::Ibc(data) => Ok(data),
            _ => Err(ContractError::new("Not an ibc chain")),
        }
    }
}
