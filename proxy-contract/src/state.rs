use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use secret_toolkit::storage::Item;


pub const EXECUTE_INSTANTIATE_REPLY_ID: u64 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct State {
    pub escrow_code_id: u64,
    pub escrow_code_hash: String,
    pub factory : String,
}

pub const STATE : Item<State> = Item::new(b"state");