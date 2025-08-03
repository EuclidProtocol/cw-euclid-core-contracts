pub mod contract;
pub mod execute;
pub mod migrate;
pub mod query;
pub mod reply;
pub mod state;
pub mod utils;

pub mod mock;
#[cfg(test)]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::ConcentratedVlpContract;
