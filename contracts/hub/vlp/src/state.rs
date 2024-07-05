use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map, SnapshotMap, Strategy};
use euclid::{
    chain::ChainUid,
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
    // Virtual Coin Contract
    pub vcoin: String,
    // Fee per swap for each transaction
    pub fee: Fee,
    // The last timestamp where the balances for each token have been updated
    pub last_updated: u64,
    // total number of LP tokens issued
    pub total_lp_tokens: Uint128,
}

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_SHARES: Map<ChainUid, Uint128> = Map::new("chain_lp_shares");

// Stores a snapshotMap in order to keep track of prices for blocks for charts and other purposes
pub const BALANCES: SnapshotMap<Token, Uint128> = SnapshotMap::new(
    "balances",
    "balances_check",
    "balances_change",
    Strategy::EveryBlock,
);
