pub mod contract;
pub mod execute;
pub mod migrate;
pub mod query;
pub mod state;

#[cfg(test)]
mod tests;

pub mod mock;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::VirtualBalanceContract;
