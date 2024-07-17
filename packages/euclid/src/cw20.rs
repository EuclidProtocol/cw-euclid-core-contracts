use crate::token::Pair;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, Uint128};

use crate::{
    chain::CrossChainUserWithLimit,
    swap::NextSwapPair,
    token::{Token, TokenWithDenom},
};

#[cw_serde]
pub enum Cw20ExecuteMsg {
    Transfer { recipient: String, amount: Uint128 },
}

// CW20 Hook Msg
#[cw_serde]
pub enum Cw20HookMsg {
    Deposit {},
    Swap {
        asset_in: TokenWithDenom,
        asset_out: Token,
        min_amount_out: Uint128,
        swaps: Vec<NextSwapPair>,
        timeout: Option<u64>,
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
        tx_id: String,
        partner_fee: Option<Decimal>,
    },
    RemoveLiquidity {
        pair: Pair,
        lp_allocation: Uint128,
        timeout: Option<u64>,
        // First element in array has highest priority
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
        tx_id: String,
    },
}
