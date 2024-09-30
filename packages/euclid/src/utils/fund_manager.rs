use std::collections::HashMap;

use cosmwasm_std::{ensure, Coin, Uint128};

use crate::error::ContractError;

pub struct FundManager {
    funds: HashMap<String, Uint128>,
}

impl FundManager {
    /// Create a new fund manager
    pub fn new(funds: &[Coin]) -> Self {
        let mut fund_manager = FundManager {
            funds: HashMap::new(),
        };
        for fund in funds {
            fund_manager.add(fund);
        }
        fund_manager
    }

    /// Get the amount of funds in the manager for a given denom
    pub fn get(&self, denom: &str) -> Option<&Uint128> {
        self.funds.get(denom)
    }

    /// Add funds to the manager
    pub fn add(&mut self, fund: &Coin) {
        *self
            .funds
            .entry(fund.denom.to_string())
            .or_insert(Uint128::zero()) += fund.amount;
    }

    //   Use funds from the manager
    pub fn use_fund(&mut self, amount: Uint128, denom: &str) -> Result<(), ContractError> {
        ensure!(
            self.funds.get(denom).unwrap() >= &amount,
            ContractError::InsufficientFunds {}
        );
        *self.funds.get_mut(denom).unwrap() -= amount;
        Ok(())
    }

    /// Validate that there are no zero funds in the manager
    pub fn validate_non_zero_funds(&self) -> Result<(), ContractError> {
        ensure!(
            self.funds.iter().all(|(_, amount)| !amount.is_zero()),
            ContractError::new("Funds cannot be zero")
        );
        Ok(())
    }

    /// Validate that there are no funds in the manager. To be used after all funds operations are done.
    pub fn validate_funds_are_empty(&self) -> Result<(), ContractError> {
        ensure!(
            self.funds.iter().all(|(_, amount)| amount.is_zero()),
            ContractError::new("Funds should be empty")
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Coin, Uint128};

    use crate::error::ContractError;

    use super::*;

    #[test]
    fn test_new() {
        let fund_manager = FundManager::new(&[Coin::new(100, "atom")]);
        assert_eq!(fund_manager.funds.get("atom"), Some(&Uint128::new(100)));
    }

    #[test]
    fn test_duplicate_funds() {
        let fund_manager = FundManager::new(&[Coin::new(100, "atom"), Coin::new(200, "atom")]);
        assert_eq!(fund_manager.get("atom"), Some(&Uint128::new(300)));
    }

    #[test]
    fn test_use_fund() {
        let mut fund_manager = FundManager::new(&[Coin::new(100, "atom")]);
        assert_eq!(fund_manager.use_fund(Uint128::new(50), "atom"), Ok(()));
        assert_eq!(fund_manager.get("atom"), Some(&Uint128::new(50)));
    }

    #[test]
    fn test_use_fund_insufficient() {
        let mut fund_manager = FundManager::new(&[Coin::new(100, "atom")]);
        assert_eq!(
            fund_manager.use_fund(Uint128::new(150), "atom"),
            Err(ContractError::InsufficientFunds {})
        );
    }

    #[test]
    fn test_validate_non_zero_funds() {
        let fund_manager = FundManager::new(&[Coin::new(100, "atom")]);
        assert_eq!(fund_manager.validate_non_zero_funds(), Ok(()));
    }

    #[test]
    fn test_validate_non_zero_funds_empty() {
        let fund_manager = FundManager::new(&[Coin::new(0, "atom")]);
        assert_eq!(
            fund_manager.validate_non_zero_funds(),
            Err(ContractError::new("Funds cannot be zero"))
        );
    }

    #[test]
    fn test_validate_funds_are_empty() {
        let fund_manager = FundManager::new(&[]);
        assert_eq!(fund_manager.validate_funds_are_empty(), Ok(()));
    }

    #[test]
    fn test_funds_are_not_empty() {
        let fund_manager = FundManager::new(&[Coin::new(100, "atom")]);
        assert_eq!(
            fund_manager.validate_funds_are_empty(),
            Err(ContractError::new("Funds should be empty"))
        );
    }

    #[test]
    fn test_validate_funds_are_empty_after_use() {
        let mut fund_manager = FundManager::new(&[Coin::new(100, "atom")]);
        fund_manager.use_fund(Uint128::new(100), "atom").unwrap();
        assert_eq!(fund_manager.validate_funds_are_empty(), Ok(()));
    }

    #[test]
    fn test_insufficient_funds() {
        let mut fund_manager = FundManager::new(&[Coin::new(100, "atom")]);
        assert_eq!(
            fund_manager.use_fund(Uint128::new(150), "atom"),
            Err(ContractError::InsufficientFunds {})
        );
    }
}
