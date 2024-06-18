use std::num::ParseIntError;

use cosmwasm_std::{DivideByZeroError, OverflowError, StdError, Uint128};

use thiserror::Error;

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

    #[error("ZeroWithdrawalAmount")]
    ZeroWithdrawalAmount {},

    #[error("InvalidWithdrawalAmount")]
    InvalidWithdrawalAmount {},

    #[error("DuplicateDenominations")]
    DuplicateDenominations {},

    #[error("UnsupportedDenomination")]
    UnsupportedDenomination {},

    #[error("UnsupportedMessage")]
    UnsupportedMessage {},

    #[error("UnsupportedOperation")]
    UnsupportedOperation {},

    #[error("Not Implemented")]
    NotImplemented {},

    #[error("DenomDoesNotExist")]
    DenomDoesNotExist {},

    #[error("Instantiate Error - {err}")]
    InstantiateError { err: String },

    #[error("Pool request already exist")]
    PoolRequestAlreadyExists {},

    #[error("Pool request {req:?} already exist")]
    PoolRequestDoesNotExists { req: String },

    #[error("Pool already created for this chain")]
    PoolAlreadyExists {},

    #[error("only unordered channels are supported")]
    OrderedChannel {},

    #[error("invalid IBC channel version. Got ({actual}), expected ({expected})")]
    InvalidVersion { actual: String, expected: String },

    #[error("Invalid Token ID")]
    InvalidTokenID {},

    #[error("Asset does not exist in VLP")]
    AssetDoesNotExist {},

    #[error("Cannot Swap 0 tokens")]
    ZeroAssetAmount {},

    #[error("DuplicateTokens")]
    DuplicateTokens {},

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

    #[error("EscrowDoesNotExist")]
    EscrowDoesNotExist {},

    #[error("EscrowAlreadyExists")]
    EscrowAlreadyExists {},

    #[error("The swap does not exist in state for the sender")]
    SwapDoesNotExist {},

    #[error("Swap already exist in state for the sender")]
    SwapAlreadyExist {},

    #[error("The deposit amount is insufficient to add the liquidity")]
    InsufficientDeposit {},

    #[error("InsufficientFunds")]
    InsufficientFunds {},

    #[error("The CHAIN ID is not valid")]
    InvalidChainId {},

    #[error("Liquity already exist in state for the sender")]
    LiquidityTxAlreadyExist {},

    #[error("Slippage has been exceeded when providing liquidity.")]
    LiquiditySlippageExceeded {},

    #[error("Pool Instantiate Failed {err}")]
    PoolInstantiateFailed { err: String },
}
