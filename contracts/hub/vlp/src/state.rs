use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{chain::ChainUid, msgs::vlp::State, token::Token};

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint128> = Map::new("balances");
