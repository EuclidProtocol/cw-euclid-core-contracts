#![allow(clippy::too_many_arguments)]

pub mod contract;
pub mod execute;
pub mod migrate;
pub mod outbound;
pub mod query;
pub mod reply;
pub mod state;

#[cfg(not(target_arch = "wasm32"))]
pub mod mock;

#[cfg(not(target_arch = "wasm32"))]
mod interface;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::interface::PoolFactoryContract;

#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
