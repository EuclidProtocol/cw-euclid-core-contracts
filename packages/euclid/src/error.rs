use cosmwasm_std::{StdError, Uint128};

use thiserror::Error;
#[derive(Error, Debug)]
pub enum Never {}
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Pool already created for this chain")]
    PoolAlreadyExists {},

    #[error("only unordered channels are supported")]
    OrderedChannel {},

    #[error("invalid IBC channel version. Got ({actual}), expected ({expected})")]
    InvalidVersion { actual: String, expected: String },

    #[error("Asset does not exist in VLP")]
    AssetDoesNotExist {},

    #[error("Cannot Swap 0 tokens")]
    ZeroAssetAmount {},

    #[error("Slippage has not been tolerated for set amount, amount: {amount}, min_amount_out: {min_amount_out}")]
    SlippageExceeded {amount: Uint128, min_amount_out: Uint128},

    #[error("Invalid Liquidity Ratio")]
    InvalidLiquidityRatio {},

    #[error("Slippage Tolerance must be between 0 and 100")]
    InvalidSlippageTolerance {},

    #[error("The Channel specified does not currently exist")]
    ChannelDoesNotExist {},

    #[error("The swap does not exist in state for the sender")]
    SwapDoesNotExist {},

    #[error("The deposit amount is insufficient to add the liquidity")]
    InsufficientDeposit {},

}
