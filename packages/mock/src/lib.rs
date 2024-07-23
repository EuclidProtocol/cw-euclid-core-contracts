#![cfg(not(target_arch = "wasm32"))]

pub use mock::MockEuclid;
pub use mock_contract::MockContract;

pub mod mock;
pub mod mock_builder;
pub mod mock_contract;
pub mod testing;
