#![allow(clippy::too_many_arguments)]

pub mod contract;
pub mod execute;
pub mod migrate;
pub mod query;
pub mod state;

#[cfg(test)]
mod testing;
