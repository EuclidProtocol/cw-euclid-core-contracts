use cosmwasm_schema::cw_serde;
use cosmwasm_std::ensure;

use crate::{chain::ChainUid, error::ContractError};

#[cw_serde]
pub struct CrossChainUser {
    pub chain_uid: ChainUid,
    pub address: String,
}

impl CrossChainUser {
    #[must_use]
    pub fn new(chain_uid: ChainUid, address: String) -> Self {
        Self { chain_uid, address }
    }

    #[must_use]
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

#[cfg(test)]
mod cross_chain_user_test {
    use super::*;
    use crate::chain::ChainUid;

    #[test]
    fn test_mixed_case_address_validate_rejects() {
        let user = CrossChainUser::new(
            ChainUid::create("cosmos".to_string()).unwrap(),
            "Cosmos1AbCdEf".to_string(),
        );
        let err = user.validate().unwrap_err();
        assert!(err.to_string().contains("Address must be lowercase"));
    }

    #[test]
    fn test_lowercase_address_validate_accepts() {
        let user = CrossChainUser::new(
            ChainUid::create("cosmos".to_string()).unwrap(),
            "cosmos1abcdef".to_string(),
        );
        user.validate().unwrap();
    }

    #[test]
    fn test_empty_address_validate_rejects() {
        let user = CrossChainUser::new(
            ChainUid::create("cosmos".to_string()).unwrap(),
            String::new(),
        );
        let err = user.validate().unwrap_err();
        assert!(err.to_string().contains("Address cannot be empty"));
    }
}
