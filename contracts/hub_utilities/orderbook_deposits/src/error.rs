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

    #[error("contract is paused")]
    ContractPaused {},

    #[error("current root not found")]
    RootNotFound {},

    #[error("pending root not found")]
    PendingRootNotFound {},

    #[error("root id mismatch")]
    RootIdMismatch {},

    #[error("root not ready for activation")]
    RootNotReady {},

    #[error("invalid root hash")]
    InvalidRootHash {},

    #[error("invalid merkle proof")]
    InvalidMerkleProof {},

    #[error("invalid leaf data")]
    InvalidLeaf {},

    #[error("permit signer not configured")]
    PermitSignerNotConfigured {},

    #[error("permit expired")]
    PermitExpired {},

    #[error("invalid permit")]
    InvalidPermit {},

    #[error("permit already used")]
    PermitAlreadyUsed {},

    #[error("withdrawal already consumed")]
    WithdrawalAlreadyConsumed {},

    #[error("insufficient withdrawable balance")]
    InsufficientWithdrawableBalance {},

    #[error("insufficient escrow balance")]
    InsufficientEscrow {},

    #[error("invalid destination")]
    InvalidDestination {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
