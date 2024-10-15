pub mod contract;
pub mod execute;
pub mod ibc;
pub mod migrate;
pub mod query;
pub mod reply;
pub mod state;
#[cfg(test)]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
pub mod mock;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::RouterContract;
