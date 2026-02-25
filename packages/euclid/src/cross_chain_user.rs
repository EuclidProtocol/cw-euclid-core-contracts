use cosmwasm_schema::cw_serde;
use cosmwasm_std::ensure;

use crate::{chain::ChainUid, error::ContractError};

#[cw_serde]
pub struct CrossChainUser {
    pub chain_uid: ChainUid,
    pub address: String,
}

impl CrossChainUser {
    pub fn new(chain_uid: ChainUid, address: String) -> Self {
        Self { chain_uid, address }
    }

    pub fn to_sender_string(&self) -> String {
        format!(
            "{chain}:{address}",
            chain = self.chain_uid.as_str(),
            address = self.address.as_str()
        )
    }

    pub fn validate(&self) -> Result<&Self, ContractError> {
        ensure!(
            !self.address.is_empty(),
            ContractError::new("Address cannot be empty")
        );
        // Ensure address is lowercase
        ensure!(
            self.address.to_lowercase() == self.address,
            ContractError::new("Address must be lowercase")
        );
        self.chain_uid.validate()?;
        Ok(self)
    }
}
