use cosmwasm_std::ensure;

use crate::error::ContractError;

/// Ensures that the timeout is between 30 and 240 seconds, and if not provided, defaults the timeout to 60 seconds
pub fn get_timeout(timeout: Option<u64>) -> Result<u64, ContractError> {
    if let Some(timeout) = timeout {
        // Validate that the timeout is between 30 and 240 seconds inclusive
        ensure!(
            timeout.ge(&30) && timeout.le(&240),
            ContractError::InvalidTimeout {}
        );
        Ok(timeout)
    } else {
        // Default timeout to 60 seconds if not provided
        Ok(60)
    }
}
