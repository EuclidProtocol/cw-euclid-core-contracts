use cw_storage_plus::{Item, Map};
use euclid::msgs::claimer::{Claim, State};

pub const STATE: Item<State> = Item::new("state");

pub const CLAIMS: Map<u128, Claim> = Map::new("claims");

// Claim id is unique incremental counter
pub const CLAIM_ID: Item<u128> = Item::new("claim_id");

// list of claims created by sender
pub const SENDER_CLAIMS: Map<String, Vec<u128>> = Map::new("sender_claims");
// List of claims against pub key
pub const USER_CLAIMS: Map<String, Vec<u128>> = Map::new("user_claims");
