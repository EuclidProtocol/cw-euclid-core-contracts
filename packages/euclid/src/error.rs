use std::num::ParseIntError;

use cosmwasm_std::{
    Addr, CheckedMultiplyRatioError, ConversionOverflowError, DivideByZeroError, OverflowError,
    StdError, Uint128,
};
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
    CheckedMultiplyFractionError(#[from] CheckedMultiplyFractionError),

    #[error("{0}")]
    CheckedMultiplyRatioError(#[from] CheckedMultiplyRatioError),

    #[error("{0}")]
    DivideByZero(#[from] DivideByZeroError),

    #[error("{0}")]
    ParseIntError(#[from] ParseIntError),

    #[error("Error - {err}")]
    Generic { err: String },

    #[error("Unreachable Code")]
    UnreachableCode {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Tx already exist")]
    TxAlreadyExist {},

    #[error("Token already exist")]
    TokenAlreadyExist {},

    #[error("Chain already exist")]
    ChainAlreadyExist {},

    #[error("Invalid Asset: {asset}")]
    InvalidAsset { asset: String },

    #[error("Zero Asset Supply: {asset}")]
    ZeroAssetSupply { asset: String },

    #[error("ZeroWithdrawalAmount")]
    ZeroWithdrawalAmount {},

    #[error("InvalidWithdrawalAmount")]
    InvalidWithdrawalAmount {},

    #[error("DuplicateDenominations")]
    DuplicateDenominations {},

    #[error("DeregisteredChain")]
    DeregisteredChain {},

    #[error("UnsupportedDenomination")]
    UnsupportedDenomination {},

    #[error("CannotEscrowVoucher")]
    CannotEscrowVoucher {},

    #[error("UnsupportedMessage")]
    UnsupportedMessage {},

    #[error("UnsupportedOperation")]
    UnsupportedOperation {},

    #[error("Not Implemented")]
    NotImplemented {},

    #[error("DenomDoesNotExist")]
    DenomDoesNotExist {},

    #[error("ChainNotFound")]
    ChainNotFound {},

    #[error("InvalidPartnerFee")]
    InvalidPartnerFee {},

    #[error("Instantiate Error - {err}")]
    InstantiateError { err: String },

    #[error("Pool request already exist")]
    PoolRequestAlreadyExists {},

    #[error("Pool request {req:?} does not exist")]
    PoolRequestDoesNotExists { req: String },

    #[error("Pool already created for this chain")]
    PoolAlreadyExists {},

    #[error("Pool doesn't exist for this chain")]
    PoolDoesNotExist {},

    #[error("only unordered channels are supported")]
    OrderedChannel {},

    #[error("invalid IBC channel version. Got ({actual}), expected ({expected})")]
    InvalidVersion { actual: String, expected: String },

    #[error("Invalid Token ID")]
    InvalidTokenID {},

    #[error("Virtual Balance Address Cannot Be Empty")]
    EmptyVirtualBalanceAddress {},

    #[error("ChannelNotFound")]
    ChannelNotFound {},

    #[error("Asset does not exist in VLP")]
    AssetDoesNotExist {},

    #[error("Cannot Swap 0 tokens")]
    ZeroAssetAmount {},

    #[error("No Channel for Local Chain")]
    NoChannelForLocalChain {},

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

    #[error("The specified channel already exists")]
    ChannelAlreadyExists {},

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

    #[error("ContractLocked")]
    ContractLocked {},

    // BEGIN CW20 ERRORS
    #[error("Cannot set to own account")]
    CannotSetOwnAccount {},

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Invalid Denom")]
    InvalidDenom {},

    #[error("Allowance is expired")]
    Expired {},

    #[error("No allowance for this account")]
    NoAllowance {},

    #[error("Minting cannot exceed the cap")]
    CannotExceedCap {},

    #[error("Logo binary data exceeds 5KB limit")]
    LogoTooBig {},

    #[error("Invalid migration. Unable to migrate from version {prev}")]
    InvalidMigration { prev: String },

    #[error("Invalid xml preamble for SVG")]
    InvalidXmlPreamble {},

    #[error("Invalid png header")]
    InvalidPngHeader {},

    #[error("Instantiate2 Address Mistmatch: expected: {expected}, received: {received}")]
    Instantiate2AddressMismatch { expected: Addr, received: Addr },

    #[error("Duplicate initial balance addresses")]
    DuplicateInitialBalanceAddresses {},

    #[error("Invalid expiration")]
    InvalidExpiration {},
    // END CW20 ERRORS
}

impl ContractError {
    pub fn new(err: &str) -> Self {
        ContractError::Generic {
            err: err.to_string(),
        }
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum Snip20ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Cannot set to own account")]
    CannotSetOwnAccount {},

    // Unused error case. Zero is now treated like every other value.
    #[deprecated(note = "Unused. All zero amount checks have been removed")]
    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Allowance is expired")]
    Expired {},

    #[error("No allowance for this account")]
    NoAllowance {},

    #[error("Minting cannot exceed the cap")]
    CannotExceedCap {},

    #[error("Logo binary data exceeds 5KB limit")]
    LogoTooBig {},

    #[error("Invalid xml preamble for SVG")]
    InvalidXmlPreamble {},

    #[error("Invalid png header")]
    InvalidPngHeader {},

    #[error("Invalid expiration value")]
    InvalidExpiration {},

    #[error("Duplicate initial balance addresses")]
    DuplicateInitialBalanceAddresses {},
}

impl From<Snip20ContractError> for ContractError {
    fn from(err: Snip20ContractError) -> Self {
        match err {
            Snip20ContractError::Std(std) => ContractError::Std(std),
            Snip20ContractError::Expired {} => ContractError::Expired {},
            Snip20ContractError::LogoTooBig {} => ContractError::LogoTooBig {},
            Snip20ContractError::NoAllowance {} => ContractError::NoAllowance {},
            Snip20ContractError::Unauthorized {} => ContractError::Unauthorized {},
            Snip20ContractError::CannotExceedCap {} => ContractError::CannotExceedCap {},
            Snip20ContractError::InvalidPngHeader {} => ContractError::InvalidPngHeader {},
            Snip20ContractError::InvalidXmlPreamble {} => ContractError::InvalidXmlPreamble {},
            Snip20ContractError::CannotSetOwnAccount {} => ContractError::CannotSetOwnAccount {},
            Snip20ContractError::DuplicateInitialBalanceAddresses {} => {
                ContractError::DuplicateInitialBalanceAddresses {}
            }
            Snip20ContractError::InvalidExpiration {} => ContractError::InvalidExpiration {},
            _ => panic!("Unsupported snip20 error: {err:?}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CheckedMultiplyFractionError {
    #[error("{_0}")]
    DivideByZero(#[from] DivideByZeroError),

    #[error("{_0}")]
    ConversionOverflow(#[from] ConversionOverflowError),

    #[error("{_0}")]
    Overflow(#[from] OverflowError),
}
