#![cfg(not(target_arch = "wasm32"))]

pub mod constants;
pub mod test_macros;

pub mod factory_add_liquidity;
pub mod factory_create_pool;
pub mod factory_full;
pub mod factory_register;
pub mod factory_register_denom;
pub mod factory_swap;
pub mod mixed_decimal_pool;
pub mod pending_packets;
pub mod state_sync;
pub mod voucher_release;


pub mod clp;
pub mod concentrated_collect;
pub mod concentrated_create_pool;
pub mod concentrated_failures;
pub mod concentrated_fees;
pub mod concentrated_positions;
pub mod concentrated_swap;
pub mod concentrated_v3_fees;
pub mod concentrated_v3_oracle;
pub mod concentrated_v3_positions;
pub mod concentrated_v3_swap;
pub mod factory_swap_mixed_concentrated;
