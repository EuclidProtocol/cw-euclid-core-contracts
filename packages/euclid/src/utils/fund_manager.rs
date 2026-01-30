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
    pub fn get(&self, denom: &str) -> Uint128 {
        self.funds.get(denom).cloned().unwrap_or(Uint128::zero())
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
            !amount.is_zero(),
            ContractError::new("Amount cannot be zero")
        );
        ensure!(
            self.get(denom).ge(&amount),
            ContractError::InsufficientFunds {}
        );
        let balance = self
            .funds
            .get_mut(denom)
            .ok_or(ContractError::new("Denom not found"))?;
        *balance = balance.checked_sub(amount)?;
        // Remove the denom if the balance is zero
        if balance.is_zero() {
            self.funds.remove(denom);
        }
        Ok(())
    }

    pub fn get_funds(&self) -> Vec<Coin> {
        self.funds
            .iter()
            .map(|(denom, amount)| Coin::new(amount.u128(), denom))
            .collect()
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

    /// Validate that there are n number of funds in the manager
    pub fn validate_n_funds(&self, n: usize) -> Result<(), ContractError> {
        ensure!(
            self.funds.len() == n,
            ContractError::new(&format!("Expected {} funds, got {}", n, self.funds.len()))
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
        let fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        assert_eq!(fund_manager.get("atom"), Uint128::new(100));
    }

    #[test]
    fn test_duplicate_funds() {
        let fund_manager =
            FundManager::new(&[Coin::new(100u128, "atom"), Coin::new(200u128, "atom")]);
        assert_eq!(fund_manager.get("atom"), Uint128::new(300));
    }

    #[test]
    fn test_use_fund() {
        let mut fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        assert_eq!(fund_manager.use_fund(Uint128::new(50), "atom"), Ok(()));
        assert_eq!(fund_manager.get("atom"), Uint128::new(50));
    }

    #[test]
    fn test_use_fund_insufficient() {
        let mut fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        assert_eq!(
            fund_manager.use_fund(Uint128::new(150), "atom"),
            Err(ContractError::InsufficientFunds {})
        );
    }

    #[test]
    fn test_validate_non_zero_funds() {
        let fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        assert_eq!(fund_manager.validate_non_zero_funds(), Ok(()));
    }

    #[test]
    fn test_validate_non_zero_funds_empty() {
        let fund_manager = FundManager::new(&[Coin::new(0u128, "atom")]);
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
        let fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        assert_eq!(
            fund_manager.validate_funds_are_empty(),
            Err(ContractError::new("Funds should be empty"))
        );
    }

    #[test]
    fn test_validate_funds_are_empty_after_use() {
        let mut fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        fund_manager.use_fund(Uint128::new(100), "atom").unwrap();
        assert_eq!(fund_manager.validate_funds_are_empty(), Ok(()));
    }

    #[test]
    fn test_insufficient_funds() {
        let mut fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        assert_eq!(
            fund_manager.use_fund(Uint128::new(150), "atom"),
            Err(ContractError::InsufficientFunds {})
        );
    }
}
