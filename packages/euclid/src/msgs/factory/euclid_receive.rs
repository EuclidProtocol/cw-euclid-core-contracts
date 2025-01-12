use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::{
    chain::{CrossChainUser, CrossChainUserWithLimit},
    fee::PartnerFee,
    swap::NextSwapPair,
    token::{Token, TokenWithDenom},
};

#[cw_serde]
pub enum FactoryEuclidReceiveHook {
    Swap {
        sender: Option<CrossChainUser>,
        asset_in: TokenWithDenom,
        asset_out: Token,
        min_amount_out: Uint128,
        swaps: Vec<NextSwapPair>,
        timeout: Option<u64>,
        cross_chain_addresses: Vec<CrossChainUserWithLimit>,
        partner_fee: Option<PartnerFee>,
    },
}
