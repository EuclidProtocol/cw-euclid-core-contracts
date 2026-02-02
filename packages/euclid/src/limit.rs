use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Uint128};

use crate::error::ContractError;

#[cw_serde]
pub enum Limit {
    LessThanOrEqual(Uint128),
    Equal(Uint128),
    GreaterThanOrEqual(Uint128),
    Dynamic(Uint128),
}

impl Limit {
    pub fn validate(&self) -> Result<(), ContractError> {
        match self {
            Limit::LessThanOrEqual(amount) => ensure!(
                amount.gt(&Uint128::zero()),
                ContractError::ZeroAssetAmount {}
            ),
            Limit::Equal(amount) => ensure!(
                amount.gt(&Uint128::zero()),
                ContractError::ZeroAssetAmount {}
            ),
            Limit::GreaterThanOrEqual(amount) => ensure!(
                amount.gt(&Uint128::zero()),
                ContractError::ZeroAssetAmount {}
            ),
            Limit::Dynamic(amount) => ensure!(amount.is_zero(), ContractError::ZeroAssetAmount {}),
        };
        Ok(())
    }

    pub fn get_equal_amount(&self) -> Result<Uint128, ContractError> {
        match self {
            Limit::Equal(amount) => Ok(*amount),
            _ => Err(ContractError::new("Limit is not equal")),
        }
    }
}
