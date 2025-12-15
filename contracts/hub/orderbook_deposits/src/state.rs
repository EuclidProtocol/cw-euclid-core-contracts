use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub enum OrderbookDepositsStatus {
    Active,
    Paused,
    Stopped,
}

pub type AssetId = String;

#[cw_serde]
pub struct State {
    // Contract admin
    pub admin: Addr,
    // status of the orderbook deposits
    pub status: OrderbookDepositsStatus,
    // Virtual balance address
    pub virtual_balance: Addr,
}

pub const STATE: Item<State> = Item::new("state");
// Tracks whether an asset is allowed for deposits.
pub const WHITELISTED_ASSETS: Map<AssetId, bool> = Map::new("whitelisted_assets");

// Aggregate deposited amount per whitelisted asset.
pub const ASSET_DEPOSITS: Map<AssetId, Uint128> = Map::new("asset_deposits");

// Per-user deposits keyed by (user address, asset id).
pub const USER_DEPOSITS: Map<(Addr, AssetId), Uint128> = Map::new("user_deposits");
