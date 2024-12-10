use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::{
    chain::{CrossChainUser, CrossChainUserWithLimit},
    fee::PartnerFee,
    swap::NextSwapPair,
    token::{Pair, Token, TokenWithDenom},
};

#[cw_serde]
pub enum FactoryCw20HookMsg {
    Deposit {
        token: Token,
        timeout: Option<u64>,
        recipient: Option<CrossChainUser>,
    },
    Swap {
        asset_in: TokenWithDenom,
        asset_out: Token,
        min_amount_out: Uint128,
        swaps: Vec<NextSwapPair>,
        timeout: Option<u64>,
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
        partner_fee: Option<PartnerFee>,
    },
    RemoveLiquidity {
        pair: Pair,
        lp_allocation: Uint128,
        timeout: Option<u64>,
        // First element in array has highest priority
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    },
}
