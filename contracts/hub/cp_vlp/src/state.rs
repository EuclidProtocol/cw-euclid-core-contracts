use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::msgs::vlp::base::State;
use euclid::{chain::ChainUid, token::Token};

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint128> = Map::new("balances");

pub const COLLATERAL_LP_TOKENS: Item<Uint128> = Item::new("collateral_lp_tokens");
