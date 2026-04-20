#![allow(clippy::too_many_arguments)]

pub mod contract;
pub mod execute;
pub mod helpers;
pub mod ibc;
pub mod migrate;
pub mod query;
pub mod rate_limit;
pub mod relay_state;
pub mod reply;
pub mod state;

#[cfg(not(target_arch = "wasm32"))]
pub mod mock;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::FactoryContract;

#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
