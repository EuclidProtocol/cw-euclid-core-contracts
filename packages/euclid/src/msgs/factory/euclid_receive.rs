use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

use crate::{
    fee::PartnerFee,
    msgs::cross_chain_config::CrossChainConfig,
    recipient::Recipient,
    swap::NextSwapPair,
    token::{Token, TokenWithDenom},
};

#[cw_serde]
pub enum FactoryEuclidReceiveHook {
    Swap {
        asset_in: TokenWithDenom,
        asset_out: Token,
        min_amount_out: Uint128,
        swaps: Vec<NextSwapPair>,
        recipients: Vec<Recipient>,
        partner_fee: Option<PartnerFee>,
        cross_chain_config: CrossChainConfig,
    },
}
