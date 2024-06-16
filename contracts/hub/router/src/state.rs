use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use euclid::{msgs::router::Chain, token::Token};

#[cw_serde]
pub struct State {
    // Contract admin
    pub admin: String,
    // Pool Code ID
    pub vlp_code_id: u64,
    pub vcoin_address: Option<Addr>,
}

pub const STATE: Item<State> = Item::new("state");

// Convert it to multi index map?
pub const VLPS: Map<(Token, Token), String> = Map::new("vlps");

// Token escrow balance on each chain
pub const ESCROW_BALANCES: Map<(Token, String), Uint128> = Map::new("escrow_balances");

pub const CHAIN_ID_TO_CHAIN: Map<String, Chain> = Map::new("chain_id_to_chain");
pub const CHANNEL_TO_CHAIN_ID: Map<String, String> = Map::new("channel_to_chain_id");
