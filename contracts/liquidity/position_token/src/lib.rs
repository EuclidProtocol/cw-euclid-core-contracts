pub mod contract;
pub mod migrate;
pub mod msg;
pub mod state;

#[cfg(test)]
mod test;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::PositionTokenContract;
