use cosmwasm_std::{Uint128, Uint256};
use cw_storage_plus::{Item, Map};
use euclid::admin::EuclidAdmin;
use euclid::chain::ChainUid;
use euclid::{
    msgs::vlp::base::{PoolKey, State},
    token::Token,
};

pub const STATE: Item<State> = Item::new("state");

pub const ADMIN: Item<EuclidAdmin> = Item::new("admin");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint256> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint256> = Map::new("balances");

pub const POOL_KEY: Item<PoolKey> = Item::new("pool_key");

pub use euclid::liquidity::{MAX_TICK, MIN_TICK};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Slot0 {
    pub sqrt_price_x96: Uint256,
    pub tick: i64,
    pub observation_index: u64,
    pub observation_cardinality: u16,
    pub observation_cardinality_next: u16,
}

pub const SLOT0: Item<Slot0> = Item::new("slot0");
pub const ACTIVE_LIQUIDITY: Item<Uint128> = Item::new("active_liquidity");
pub const FEE_GROWTH_GLOBAL_0_X128: Item<Uint256> = Item::new("fee_growth_global_0_x128");
pub const FEE_GROWTH_GLOBAL_1_X128: Item<Uint256> = Item::new("fee_growth_global_1_x128");
pub const PROTOCOL_FEES_0: Item<Uint128> = Item::new("protocol_fees_0");
pub const PROTOCOL_FEES_1: Item<Uint128> = Item::new("protocol_fees_1");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TickInfo {
    pub initialized: bool,
    pub liquidity_gross: Uint128,
    pub liquidity_net: i128,
    pub fee_growth_outside_0_x128: Uint256,
    pub fee_growth_outside_1_x128: Uint256,
}

pub const TICKS: Map<i64, TickInfo> = Map::new("ticks");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Observation {
    pub block_timestamp: u64,
    pub tick_cumulative: i128,
    pub seconds_per_liquidity_cumulative_x128: Uint256,
    pub initialized: bool,
}

pub const OBSERVATIONS: Map<u64, Observation> = Map::new("observations");

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ConcentratedPosition {
    pub chain_uid: ChainUid,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub liquidity: Uint128,
    #[serde(default)]
    pub fee_growth_inside_0_last_x128: Uint256,
    #[serde(default)]
    pub fee_growth_inside_1_last_x128: Uint256,
    #[serde(default)]
    pub tokens_owed_0: Uint128,
    #[serde(default)]
    pub tokens_owed_1: Uint128,
}

pub const POSITIONS: Map<u128, ConcentratedPosition> = Map::new("positions");
