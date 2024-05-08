use cosmwasm_schema::cw_serde;
use euclid::{swap::SwapInfo, token::{Pair, PairInfo}};


use cosmwasm_std::{Deps, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct State {
    // Store VLP contract address on VLS
    pub vlp_contract: String,
    // Factory Contract
    pub factory_contract: String,
    // Token pair
    pub pair: Pair,
    // Token Pair Info
    pub pair_info: PairInfo,
    // Total cumulative reserves of token_1 in the pool
    // DOES NOT AFFECT SWAP CALCULATIONS    
    pub reserve_1: Uint128,
    // Total cumulative reserves of token_2 in the pool
    // DOES NOT AFFECT SWAP CALCULATIONS    
    pub reserve_2: Uint128,
    // Store chain Identifier (from factory)
    // The chain IDENTIFIER 'chain_id' does not need to match the chain_id of the chain the contracts are deployed on
    pub chain_id: String,
}

pub const STATE: Item<State> = Item::new("state");



/// (channel_id) -> count. Reset on channel closure.
pub const CONNECTION_COUNTS: Map<String, u32> = Map::new("connection_counts");
/// (channel_id) -> timeout_count. Reset on channel closure.
pub const TIMEOUT_COUNTS: Map<String, u32> = Map::new("timeout_count");

// Map for pending swaps for user 
pub const PENDING_SWAPS: Map<String, Vec<SwapInfo>> = Map::new("pending_swaps");

// Helper function to iterate through vector in PendingSwap map to find a certain swap_id
pub fn find_swap_id(swap_id: &str, pending_swaps: Vec<SwapInfo>) -> bool {
    for swap in pending_swaps {
        if swap.swap_id == swap_id {
            return true
        }
    }
    return false
}

// Returns SwapInfo for a specific SwapID
pub fn get_swap_info(swap_id: &str, pending_swaps: Vec<SwapInfo>) -> SwapInfo {
    for swap in pending_swaps {
        if swap.swap_id == swap_id {
            return swap
        }
    }
    panic!("Swap ID not found")
}