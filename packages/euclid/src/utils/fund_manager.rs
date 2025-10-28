use std::collections::HashMap;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Coin, Uint128};

use crate::error::ContractError;

#[cw_serde]
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
        *self
            .funds
            .get_mut(denom)
            .ok_or(ContractError::new("Denom not found"))? -= amount;
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

    /// Validate that there are n number of funds in the manager
    pub fn validate_n_funds(&self, n: usize) -> Result<(), ContractError> {
        ensure!(
            self.funds.len() == n,
            ContractError::new(&format!("Expected {} funds, got {}", n, self.funds.len()))
        );
        Ok(())
    }

    /// Get the only (denom, amount) pair in the manager. Errors if there is not exactly one fund.
    pub fn get_single_fund(&self) -> Result<(String, Uint128), ContractError> {
        if self.funds.len() != 1 {
            return Err(ContractError::new(&format!(
                "Expected exactly one fund, got {}",
                self.funds.len()
            )));
        }
        let (denom, amount) = self.funds.iter().next().unwrap();
        Ok((denom.clone(), *amount))
    }

    pub fn get_all_funds(&self) -> Vec<Coin> {
        self.funds
            .iter()
            .map(|(denom, amount)| Coin::new(amount.u128(), denom.clone()))
            .collect()
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

    #[test]
    fn test_get_single_fund_success() {
        let fund_manager = FundManager::new(&[Coin::new(100u128, "atom")]);
        let res = fund_manager.get_single_fund();
        assert_eq!(res, Ok(("atom".to_string(), Uint128::new(100))));
    }

    #[test]
    fn test_get_single_fund_error_none() {
        let fund_manager = FundManager::new(&[]);
        let res = fund_manager.get_single_fund();
        assert!(res.is_err());
    }

    #[test]
    fn test_get_single_fund_error_multiple() {
        let fund_manager =
            FundManager::new(&[Coin::new(100u128, "atom"), Coin::new(50u128, "osmo")]);
        let res = fund_manager.get_single_fund();
        assert!(res.is_err());
    }

    #[test]
    fn test_get_all_funds_multiple() {
        let fund_manager =
            FundManager::new(&[Coin::new(100u128, "atom"), Coin::new(50u128, "osmo")]);
        let mut all_funds = fund_manager.get_all_funds();
        all_funds.sort_by(|a, b| a.denom.cmp(&b.denom));
        assert_eq!(all_funds.len(), 2);
        assert_eq!(all_funds[0], Coin::new(100u128, "atom"));
        assert_eq!(all_funds[1], Coin::new(50u128, "osmo"));
    }

    #[test]
    fn test_get_all_funds_single() {
        let fund_manager = FundManager::new(&[Coin::new(999u128, "ustars")]);
        let all_funds = fund_manager.get_all_funds();
        assert_eq!(all_funds.len(), 1);
        assert_eq!(all_funds[0], Coin::new(999u128, "ustars"));
    }

    #[test]
    fn test_get_all_funds_empty() {
        let fund_manager = FundManager::new(&[]);
        let all_funds = fund_manager.get_all_funds();
        assert_eq!(all_funds, Vec::<Coin>::new());
    }
}
