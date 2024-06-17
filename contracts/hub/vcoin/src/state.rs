use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, SnapshotMap, Strategy};
use euclid::{msgs::vcoin::State, vcoin::SerializedBalanceKey};

pub const STATE: Item<State> = Item::new("state");

// Stores a snapshotMap in order to keep track of prices for blocks for debug, charts and other purposes
pub const SNAPSHOT_BALANCES: SnapshotMap<SerializedBalanceKey, Uint128> = SnapshotMap::new(
    "snapshot_balances",
    "snapshot_balances_check",
    "snapshot_balances_change",
    Strategy::EveryBlock,
);
