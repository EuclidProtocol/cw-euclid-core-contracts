#![allow(clippy::too_many_arguments)]

pub mod contract;
pub mod migrate;
pub mod query;

#[cfg(test)]
mod tests;

#[cfg(not(target_arch = "wasm32"))]
pub mod mock;
