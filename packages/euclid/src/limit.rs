use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Uint256};

use crate::error::ContractError;

#[cw_serde]
pub enum Limit {
    LessThanOrEqual(Uint256),
    Equal(Uint256),
    GreaterThanOrEqual(Uint256),
    Dynamic(Uint256),
}

impl Limit {
    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            Limit::LessThanOrEqual(amount) => ensure!(
                amount.gt(&Uint256::zero()),
                ContractError::ZeroAssetAmount {}
            ),
            Limit::Equal(amount) => ensure!(
                amount.gt(&Uint256::zero()),
                ContractError::ZeroAssetAmount {}
            ),
            Limit::GreaterThanOrEqual(_amount) => {}
            Limit::Dynamic(amount) => ensure!(
                amount.is_zero(),
                ContractError::new("Dynamic limit must have zero amount")
            ),
        };
        Ok(())
    }

    pub fn get_equal_amount(&self) -> Result<Uint256, ContractError> {
        match self {
            Limit::Equal(amount) => Ok(*amount),
            _ => Err(ContractError::new("Limit is not equal")),
        }
    }
}
