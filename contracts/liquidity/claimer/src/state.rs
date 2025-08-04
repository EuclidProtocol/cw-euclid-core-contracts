use cw_storage_plus::{Item, Map};
use euclid::msgs::claimer::{Claim, State};

pub const STATE: Item<State> = Item::new("state");

pub const CLAIMS: Map<u128, Claim> = Map::new("claims");

// Claim id is unique incremental counter
pub const CLAIM_ID: Item<u128> = Item::new("claim_id");
