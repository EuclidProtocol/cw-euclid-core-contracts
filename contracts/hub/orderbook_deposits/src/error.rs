use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Euclid(#[from] euclid::error::ContractError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("asset not whitelisted")]
    AssetNotWhitelisted {},

    #[error("invalid amount")]
    InvalidAmount {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
