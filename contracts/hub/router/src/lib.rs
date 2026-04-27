pub mod contract;
pub mod execute;
pub mod helpers;
pub mod ibc;
pub mod migrate;
pub mod query;
pub mod relay_state;
pub mod reply;
pub mod state;
#[cfg(test)]
mod testing;

#[cfg(not(target_arch = "wasm32"))]
pub mod mock;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::RouterContract;
