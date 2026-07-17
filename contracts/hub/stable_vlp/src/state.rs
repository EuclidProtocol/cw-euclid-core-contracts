use cosmwasm_std::{Uint256, Uint64};
use cw_storage_plus::{Item, Map};
use euclid::admin::EuclidAdmin;
use euclid::msgs::vlp::base::State;
use euclid::{chain::ChainUid, token::Token};

pub const STATE: Item<State> = Item::new("state");
pub const ADMIN: Item<EuclidAdmin> = Item::new("admin");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint256> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint256> = Map::new("balances");

// The amplification factor for the stableswap invariant, default is 1000
pub const AMP_FACTOR: Item<Uint64> = Item::new("amp_factor");

pub const COLLATERAL_LP_TOKENS: Item<Uint256> = Item::new("collateral_lp_tokens");
