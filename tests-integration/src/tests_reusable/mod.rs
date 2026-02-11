#![cfg(not(target_arch = "wasm32"))]

pub mod factory_add_liquidity;
pub mod factory_create_pool;
pub mod factory_full;
pub mod factory_register;
pub mod factory_register_denom;
pub mod factory_swap;
pub mod state_sync;
