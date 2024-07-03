use std::ops::Deref;

use cosmwasm_schema::cw_serde;

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
        for c in self.0.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '.' {
                return Err(ContractError::new(
                    "Invalid UID format: must be lowercase, alphanumeric or '.'",
                ));
            }
        }
        Ok(self)
    }
}

#[cw_serde]
pub struct CrossChainUser {
    pub chain_uid: ChainUid,
    pub address: String,
}
