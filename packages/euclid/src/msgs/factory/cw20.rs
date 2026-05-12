use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint256;

use crate::{
    cross_chain_user::CrossChainUser,
    fee::PartnerFee,
    msgs::{cross_chain_config::CrossChainConfig, hook::EuclidReceive},
    recipient::Recipient,
    swap::NextSwapPair,
    token::{Pair, Token, TokenWithDenom},
};

#[cw_serde]
pub enum FactoryCw20HookMsg {
    Deposit {
        token: Token,
        recipients: Vec<Recipient>,
        cross_chain_config: CrossChainConfig,
    },
    RemoveLiquidity {
        pair: Pair,
        recipient: CrossChainUser,
        cross_chain_config: CrossChainConfig,
    },
    Swap {
        asset_in: TokenWithDenom,
        asset_out: Token,
        min_amount_out: Uint256,
        swaps: Vec<NextSwapPair>,
        recipients: Vec<Recipient>,
        partner_fee: Option<PartnerFee>,
        cross_chain_config: CrossChainConfig,
    },
    EuclidReceive(EuclidReceive),
}
