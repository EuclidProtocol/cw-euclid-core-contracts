pub mod contract;
mod error;
pub mod execute;
pub mod helpers;
pub mod migrate;
pub mod query;

pub mod state;

pub use crate::error::ContractError;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::OrderbookDepositsContract;
