pub mod contract;
pub mod execute;
pub mod integration_tests;
pub mod migrate;
pub mod query;
pub mod state;

mod test;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::Cw20Contract;
