use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Uint128};
use cw_storage_plus::{Item, Map};

use euclid::msgs::orderbook_deposits::{AssetTotal, OrderbookDepositsStatus};
pub type AssetId = String;

#[cw_serde]
pub struct State {
    // status of the orderbook deposits
    pub status: OrderbookDepositsStatus,
    // Virtual balance address
    pub virtual_balance: Addr,
}

pub const ADMIN: Item<Addr> = Item::new("admin");

#[cw_serde]
pub struct RootConfig {
    pub permit_signer_pubkey: Option<Binary>,
    pub permit_signer_address: Option<String>,
    pub root_challenge_period: u64,
    pub authorized_posters: Vec<Addr>,
}

#[cw_serde]
pub struct RootInfo {
    pub root_id: String,
    pub root_hash: Binary,
    pub per_asset_totals: Vec<AssetTotal>,
    pub da_hash: Option<Binary>,
    pub da_url: Option<String>,
    pub proposed_at: u64,
}

pub const STATE: Item<State> = Item::new("state");
pub const ROOT_CONFIG: Item<RootConfig> = Item::new("root_config");
pub const CURRENT_ROOT: Item<RootInfo> = Item::new("current_root");
pub const PENDING_ROOT: Item<RootInfo> = Item::new("pending_root");
// Tracks whether an asset is allowed for deposits.
pub const WHITELISTED_ASSETS: Map<AssetId, bool> = Map::new("whitelisted_assets");

// Aggregate deposited amount per whitelisted asset.
pub const ASSET_DEPOSITS: Map<AssetId, Uint128> = Map::new("asset_deposits");

// Per-user deposits keyed by (user address, asset id).
pub const USER_DEPOSITS: Map<(String, AssetId), Uint128> = Map::new("user_deposits");

// Tracks withdrawn amounts by hashed (root_id, user, asset, nonce).
pub const NULLIFIERS: Map<String, Uint128> = Map::new("nullifiers");

// Prevents permit replay by storing a hash of the signed permit data.
pub const USED_PERMITS: Map<String, bool> = Map::new("used_permits");
