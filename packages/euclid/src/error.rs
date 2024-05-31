use std::num::ParseIntError;

use cosmwasm_std::{DivideByZeroError, OverflowError, StdError, Uint128};

use thiserror::Error;

use crate::{liquidity::LiquidityTxInfo, pool::PoolRequest, swap::SwapInfo};
#[derive(Error, Debug)]
pub enum Never {}
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Overflow(#[from] OverflowError),

    #[error("{0}")]
    DivideByZero(#[from] DivideByZeroError),

    #[error("{0}")]
    ParseIntError(#[from] ParseIntError),

    #[error("Error - {err}")]
    Generic { err: String },

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Pool request {req:?} already exist")]
    PoolRequestAlreadyExists { req: PoolRequest },

    #[error("Pool request {req:?} already exist")]
    PoolRequestDoesNotExists { req: String },

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

    #[error("Zero Slippage Amount")]
    ZeroSlippageAmount {},

    #[error("Slippage has not been tolerated for set amount, amount: {amount}, min_amount_out: {min_amount_out}")]
    SlippageExceeded {
        amount: Uint128,
        min_amount_out: Uint128,
    },

    #[error("Invalid Liquidity Ratio")]
    InvalidLiquidityRatio {},

    #[error("Invalid Timeout")]
    InvalidTimeout {},

    #[error("Slippage Tolerance must be between 0 and 100")]
    InvalidSlippageTolerance {},

    #[error("The Channel specified does not currently exist")]
    ChannelDoesNotExist {},

    #[error("The swap does not exist in state for the sender")]
    SwapDoesNotExist {},

    #[error("The swap - {req:?} already exist in state for the sender")]
    SwapAlreadyExist { req: SwapInfo },

    #[error("The deposit amount is insufficient to add the liquidity")]
    InsufficientDeposit {},

    #[error("The CHAIN ID is not valid")]
    InvalidChainId {},

    #[error("The liquity tx - {req:?} already exist in state for the sender")]
    LiquidityTxAlreadyExist { req: LiquidityTxInfo },

    #[error("Slippage has been exceeded when providing liquidity.")]
    LiquiditySlippageExceeded {},

    #[error("Pool Instantiate Failed {err}")]
    PoolInstantiateFailed { err: String },
}
