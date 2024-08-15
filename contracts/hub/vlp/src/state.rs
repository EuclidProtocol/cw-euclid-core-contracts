use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::ChainUid,
    fee::{Fee, TotalFees},
    token::{Pair, Token},
};

#[cw_serde]
pub struct State {
    // Token Pair Info
    pub pair: Pair,
    // Router Contract
    pub router: String,
    // Virtual Coin Contract
    pub vcoin: String,
    // Fee per swap for each transaction
    pub fee: Fee,
    // Total lp and euclid fees collected
    pub total_fees_collected: TotalFees,
    // The last timestamp where the balances for each token have been updated
    pub last_updated: u64,
    // total number of LP tokens issued
    pub total_lp_tokens: Uint128,
    pub admin: String,
}

pub const STATE: Item<State> = Item::new("state");

pub const CHAIN_LP_TOKENS: Map<ChainUid, Uint128> = Map::new("chain_lp_tokens");

pub const BALANCES: Map<Token, Uint128> = Map::new("balances");
