//! Sample builders for `RouterReceiveMsg`, one per variant plus a handful of
//! edge-case builders (empty and multi-element vecs, `Some`/`None` options,
//! negative and positive tick indexes, `Uint256::MAX`). Shared domain values
//! are reused from `types_samples` where sensible.

use cosmwasm_std::{Uint128, Uint256};
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::vlp::base::PoolKey;
use euclid::recipient::Recipient;
use euclid::swap::NextSwapPair;
use euclid::token::{Pair, PairWithDenomAndAmount, TokenWithDenom};
use euclid_ibc::wire::envelope::router::RouterReceiveMsg;
use euclid_ibc::wire::msgs::{
    AddConcentratedLiquiditySendMsg, AddLiquiditySendMsg, CollectConcentratedFeesSendMsg,
    CollectConcentratedProtocolFeesSendMsg, DepositTokenSendMsg, DeregisterDenomSendMsg,
    RegisterDenomSendMsg, RemoveConcentratedLiquiditySendMsg, RemoveLiquiditySendMsg,
    RequestConcentratedPoolCreationSendMsg, RequestPoolCreationSendMsg,
    SingleSidedAddLiquiditySendMsg, SwapSendMsg, TransferVoucherSendMsg,
};

use super::types_samples::{
    cross_chain_user_samples, next_swap_pair_samples, pair_samples,
    pair_with_denom_and_amount_samples, pool_config_samples, pool_key_samples, recipient_samples,
    token, token_with_denom_samples,
};

fn ccu() -> CrossChainUser {
    cross_chain_user_samples().into_iter().next().unwrap()
}

fn ccu_other() -> CrossChainUser {
    cross_chain_user_samples().into_iter().nth(1).unwrap()
}

fn pair() -> Pair {
    pair_samples().into_iter().next().unwrap()
}

fn pda() -> PairWithDenomAndAmount {
    pair_with_denom_and_amount_samples()
        .into_iter()
        .next()
        .unwrap()
}

fn token_with_denom() -> TokenWithDenom {
    token_with_denom_samples().into_iter().next().unwrap()
}

fn concentrated_pool_key() -> PoolKey {
    // Index 2 of the sample vector is the `Concentrated` variant.
    pool_key_samples().into_iter().nth(2).unwrap()
}

fn recipients() -> Vec<Recipient> {
    recipient_samples()
}

fn swaps() -> Vec<NextSwapPair> {
    next_swap_pair_samples()
}

pub fn register_denom_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::RegisterDenom(RegisterDenomSendMsg {
        sender: ccu(),
        tx_id: "tx-register".to_string(),
        token: token_with_denom(),
    })
}

pub fn deregister_denom_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::DeregisterDenom(DeregisterDenomSendMsg {
        sender: ccu(),
        tx_id: "tx-deregister".to_string(),
        token: token_with_denom(),
    })
}

pub fn transfer_voucher_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::TransferVoucher(TransferVoucherSendMsg {
        sender: ccu(),
        token: token("abc"),
        amount: Uint256::MAX,
        from: Some(ccu_other()),
        recipients: recipients(),
        tx_id: "tx-transfer".to_string(),
    })
}

pub fn transfer_voucher_empty_recipients_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::TransferVoucher(TransferVoucherSendMsg {
        sender: ccu(),
        token: token("abc"),
        amount: Uint256::zero(),
        from: None,
        recipients: vec![],
        tx_id: "tx-transfer-empty".to_string(),
    })
}

pub fn deposit_token_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::DepositToken(DepositTokenSendMsg {
        sender: ccu(),
        asset_in: token_with_denom(),
        amount_in: Uint256::from(123_456u128),
        recipients: recipients(),
        tx_id: "tx-deposit".to_string(),
    })
}

pub fn request_pool_creation_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::RequestPoolCreation(RequestPoolCreationSendMsg {
        sender: ccu(),
        tx_id: "tx-request-pool".to_string(),
        pair: pda(),
        pool_config: pool_config_samples().into_iter().next().unwrap(),
        slippage_tolerance_bps: 50,
    })
}

pub fn request_concentrated_pool_creation_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::RequestConcentratedPoolCreation(RequestConcentratedPoolCreationSendMsg {
        sender: ccu(),
        tx_id: "tx-request-clp".to_string(),
        pair: pda(),
        pool_key: concentrated_pool_key(),
        slippage_tolerance_bps: 30,
        initial_tick: Some(-42),
    })
}

pub fn request_concentrated_pool_creation_no_tick_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::RequestConcentratedPoolCreation(RequestConcentratedPoolCreationSendMsg {
        sender: ccu(),
        tx_id: "tx-request-clp-no-tick".to_string(),
        pair: pda(),
        pool_key: concentrated_pool_key(),
        slippage_tolerance_bps: 0,
        initial_tick: None,
    })
}

pub fn add_liquidity_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::AddLiquidity(AddLiquiditySendMsg {
        sender: ccu(),
        slippage_tolerance_bps: 100,
        pair: pda(),
        tx_id: "tx-add-liq".to_string(),
    })
}

pub fn add_concentrated_liquidity_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::AddConcentratedLiquidity(AddConcentratedLiquiditySendMsg {
        sender: ccu(),
        pair: pda(),
        pool_key: concentrated_pool_key(),
        lower_tick_index: -887_272,
        upper_tick_index: 887_272,
        position_id: Some(Uint128::new(7)),
        slippage_tolerance_bps: 25,
        tx_id: "tx-add-clp".to_string(),
    })
}

pub fn add_concentrated_liquidity_no_position_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::AddConcentratedLiquidity(AddConcentratedLiquiditySendMsg {
        sender: ccu(),
        pair: pda(),
        pool_key: concentrated_pool_key(),
        lower_tick_index: -100,
        upper_tick_index: 200,
        position_id: None,
        slippage_tolerance_bps: 25,
        tx_id: "tx-add-clp-new".to_string(),
    })
}

pub fn remove_liquidity_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::RemoveLiquidity(RemoveLiquiditySendMsg {
        sender: ccu(),
        lp_allocation: Uint256::MAX,
        pair: pair(),
        recipient: ccu_other(),
        tx_id: "tx-remove-liq".to_string(),
    })
}

pub fn remove_concentrated_liquidity_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::RemoveConcentratedLiquidity(RemoveConcentratedLiquiditySendMsg {
        sender: ccu(),
        pool_key: concentrated_pool_key(),
        position_id: Uint128::new(9),
        liquidity_delta: Uint128::MAX,
        recipient: ccu_other(),
        tx_id: "tx-remove-clp".to_string(),
    })
}

pub fn collect_concentrated_fees_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::CollectConcentratedFees(CollectConcentratedFeesSendMsg {
        sender: ccu(),
        pool_key: concentrated_pool_key(),
        position_id: Uint128::new(3),
        recipient: ccu_other(),
        tx_id: "tx-collect-fees".to_string(),
    })
}

pub fn collect_concentrated_protocol_fees_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::CollectConcentratedProtocolFees(CollectConcentratedProtocolFeesSendMsg {
        sender: ccu(),
        pool_key: concentrated_pool_key(),
        recipient: ccu_other(),
        amount_0_requested: Uint128::zero(),
        amount_1_requested: Uint128::MAX,
        tx_id: "tx-collect-protocol-fees".to_string(),
    })
}

pub fn swap_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::Swap(SwapSendMsg {
        sender: ccu(),
        asset_in: token_with_denom(),
        amount_in: Uint256::MAX,
        asset_out: token("out"),
        min_amount_out: Uint256::from(1u128),
        swaps: swaps(),
        recipients: recipients(),
        partner_fee_amount: Uint256::from(999u128),
        partner_fee_recipient: ccu_other(),
        tx_id: "tx-swap".to_string(),
    })
}

pub fn swap_single_hop_empty_recipients_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::Swap(SwapSendMsg {
        sender: ccu(),
        asset_in: token_with_denom(),
        amount_in: Uint256::from(10u128),
        asset_out: token("out"),
        min_amount_out: Uint256::zero(),
        swaps: swaps().into_iter().take(1).collect(),
        recipients: vec![],
        partner_fee_amount: Uint256::zero(),
        partner_fee_recipient: ccu(),
        tx_id: "tx-swap-single".to_string(),
    })
}

pub fn single_sided_add_liquidity_msg() -> RouterReceiveMsg {
    RouterReceiveMsg::SingleSidedAddLiquidity(SingleSidedAddLiquiditySendMsg {
        sender: ccu(),
        asset_in: token_with_denom(),
        amount_in: Uint256::from(500u128),
        swap_amount: Uint256::from(250u128),
        pair: pair(),
        swaps: swaps().into_iter().take(1).collect(),
        min_lp_out: Uint256::from(1u128),
        partner_fee_amount: Uint256::MAX,
        partner_fee_recipient: ccu_other(),
        tx_id: "tx-single-sided".to_string(),
    })
}

/// Every builder, labeled, for table-driven roundtrip iteration. Includes the
/// 14 base variants plus the option/vec edge-case builders.
pub fn all_samples() -> Vec<(&'static str, RouterReceiveMsg)> {
    vec![
        ("register_denom", register_denom_msg()),
        ("deregister_denom", deregister_denom_msg()),
        ("transfer_voucher", transfer_voucher_msg()),
        (
            "transfer_voucher_empty_recipients",
            transfer_voucher_empty_recipients_msg(),
        ),
        ("deposit_token", deposit_token_msg()),
        ("request_pool_creation", request_pool_creation_msg()),
        (
            "request_concentrated_pool_creation",
            request_concentrated_pool_creation_msg(),
        ),
        (
            "request_concentrated_pool_creation_no_tick",
            request_concentrated_pool_creation_no_tick_msg(),
        ),
        ("add_liquidity", add_liquidity_msg()),
        (
            "add_concentrated_liquidity",
            add_concentrated_liquidity_msg(),
        ),
        (
            "add_concentrated_liquidity_no_position",
            add_concentrated_liquidity_no_position_msg(),
        ),
        ("remove_liquidity", remove_liquidity_msg()),
        (
            "remove_concentrated_liquidity",
            remove_concentrated_liquidity_msg(),
        ),
        ("collect_concentrated_fees", collect_concentrated_fees_msg()),
        (
            "collect_concentrated_protocol_fees",
            collect_concentrated_protocol_fees_msg(),
        ),
        ("swap", swap_msg()),
        (
            "swap_single_hop_empty_recipients",
            swap_single_hop_empty_recipients_msg(),
        ),
        (
            "single_sided_add_liquidity",
            single_sided_add_liquidity_msg(),
        ),
    ]
}
