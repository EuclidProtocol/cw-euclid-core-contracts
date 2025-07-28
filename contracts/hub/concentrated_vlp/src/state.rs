use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128, Uint64};
use cw_asset::AssetInfo;
use cw_storage_plus::{Item, Map, SnapshotMap};
use euclid::msgs::concentrated_vlp::{FeeShareConfig, PairInfo, PoolParams, PoolState};
use euclid::pool::State;
use euclid::{chain::ChainUid, token::Token};

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint128> = Map::new("balances");

// The amplification factor for the stableswap invariant, default is 1000
pub const AMP_FACTOR: Item<Uint64> = Item::new("amp_factor");

pub const COLLATERAL_LP_TOKENS: Item<Uint128> = Item::new("collateral_lp_tokens");

/// Concentrated VLP Config
/// Stores pool parameters and state.
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores asset balances to query them later at any block height
pub const CONCENTRATED_BALANCES: SnapshotMap<&AssetInfo, Uint128> = SnapshotMap::new(
    "balances",
    "balances_check",
    "balances_change",
    cw_storage_plus::Strategy::EveryBlock,
);
/// This structure stores the concentrated pair parameters.
#[cw_serde]
pub struct Config {
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract updated the asset cumulative prices
    pub block_time_last: u64,
    /// The vector contains cumulative prices for each pair of assets in the pool
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    /// Pool parameters
    pub pool_params: PoolParams,
    /// Pool state
    pub pool_state: PoolState,
    /// Pool's owner
    pub owner: Option<Addr>,
    /// Whether asset balances are tracked over blocks or not.
    pub track_asset_balances: bool,
    /// The config for swap fee sharing
    pub fee_share: Option<FeeShareConfig>,
    /// The tracker contract address
    pub tracker_addr: Option<Addr>,
}
