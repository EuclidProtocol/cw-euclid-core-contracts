use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal256, Uint128};
use cw_storage_plus::{Item, Map, SnapshotMap, Strategy};
use euclid::{
    fee::Fee,
    pool::Pool,
    token::{Pair, Token},
};

#[cw_serde]
pub struct State {
    // Token Pair Info
    pub pair: Pair,
    // Router Contract
    pub router: String,
    // Fee per swap for each transaction
    pub fee: Fee,
    // The last timestamp where the balances for each token have been updated
    pub last_updated: u64,
    // Total cumulative reserves of token_1
    pub total_reserve_1: Uint128,
    // Total cumulative reserves of token_2
    pub total_reserve_2: Uint128,
    // total number of LP tokens issued
    pub total_lp_tokens: Uint128,

    // Pool ratio is always constant
    pub lq_ratio: Decimal256,
}

pub const STATE: Item<State> = Item::new("state");

// A map of chain-ids connected to the VLP to pools
pub const POOLS: Map<&String, Pool> = Map::new("pools");

// Stores a snapshotMap in order to keep track of prices for blocks for charts and other purposes
pub const BALANCES: SnapshotMap<Token, Uint128> = SnapshotMap::new(
    "balances",
    "balances_check",
    "balances_change",
    Strategy::EveryBlock,
);

/// (channel_id) -> count. Reset on channel closure.
pub const CONNECTION_COUNTS: Map<String, u32> = Map::new("connection_counts");
/// (channel_id) -> timeout_count. Reset on channel closure.
pub const TIMEOUT_COUNTS: Map<String, u32> = Map::new("timeout_count");
