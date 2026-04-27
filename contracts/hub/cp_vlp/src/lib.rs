pub mod contract;
pub mod migrate;
pub mod query;
pub mod reply;
pub mod state;

pub mod mock;
#[cfg(test)]
mod testing;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::VlpContract;
