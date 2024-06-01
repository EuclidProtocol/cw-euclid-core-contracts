use cosmwasm_schema::cw_serde;
use cw_storage_plus::{Item, Map};
use euclid::{msgs::router::Chain, token::Token};

#[cw_serde]
pub struct State {
    // Contract admin
    pub admin: String,
    // Pool Code ID
    pub vlp_code_id: u64,
}

pub const STATE: Item<State> = Item::new("state");

// Convert it to multi index map?
pub const VLPS: Map<(Token, Token), String> = Map::new("vlps");

pub const CHAIN_ID_TO_CHAIN: Map<String, Chain> = Map::new("chains");
pub const CHANNEL_TO_CHAIN_ID: Map<String, String> = Map::new("channels_to_chain_id");
