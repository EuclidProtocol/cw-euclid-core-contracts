//! Sample builders for the 15 typed acknowledgement payloads plus the
//! sentinel success ack (plan §2.3/§6.4). Reuses `types_samples` builders for
//! nested `CrossChainUser`/`PoolKey`/`PairWithAmount` fields rather than
//! duplicating them.
//!
//! These builders construct the DOMAIN response structs. Roundtrip suites
//! convert each to its wire ack mirror (`AddLiquidityAckMsg::from(resp)`, ...)
//! before wrapping in `AcknowledgementMsg` and encoding.
//!
//! `tests/common` is one shared module tree compiled fresh into every
//! integration-test binary; a helper used by one binary and not another is
//! legitimately "unused" from that binary's point of view, so dead-code is
//! allowed at the module level rather than per function.
#![allow(dead_code)]

use cosmwasm_std::{Uint128, Uint256};
use euclid::cross_chain_user::CrossChainUser;
use euclid::deposit::DepositTokenResponse;
use euclid::liquidity::{
    AddLiquidityResponse, ConcentratedAddLiquidityResponse, ConcentratedCollectFeesResponse,
    ConcentratedCollectProtocolFeesResponse, ConcentratedRemoveLiquidityResponse,
    RemoveLiquidityResponse,
};
use euclid::msgs::factory::msg::{RegisterFactoryResponse, ReleaseEscrowResponse};
use euclid::msgs::vlp::base::{
    ConcentratedPoolCreationResponse, DeregisterDenomResponse, PoolCreationResponse,
    RegisterDenomResponse,
};
use euclid::swap::{SwapResponse, TransferVoucherResponse};

use super::types_samples::{chain_uid, pair_with_amount_samples, pool_key_samples, token};

fn ccu(chain: &str, address: &str) -> CrossChainUser {
    CrossChainUser::new(chain_uid(chain), address.to_string())
}

pub fn register_factory_response_samples() -> Vec<RegisterFactoryResponse> {
    vec![
        RegisterFactoryResponse {
            factory_address: "cosmos1factory".to_string(),
            chain_id: "cosmoshub-4".to_string(),
        },
        RegisterFactoryResponse {
            factory_address: String::new(),
            chain_id: String::new(),
        },
    ]
}

pub fn release_escrow_response_samples() -> Vec<ReleaseEscrowResponse> {
    vec![
        ReleaseEscrowResponse {
            amount: Uint256::MAX,
            to_address: "cosmos1recipient".to_string(),
            escrow_balance: Uint256::zero(),
        },
        ReleaseEscrowResponse {
            amount: Uint256::zero(),
            to_address: "0xrecipient".to_string(),
            escrow_balance: Uint256::MAX,
        },
    ]
}

pub fn register_denom_response_samples() -> Vec<RegisterDenomResponse> {
    vec![RegisterDenomResponse {}]
}

pub fn deregister_denom_response_samples() -> Vec<DeregisterDenomResponse> {
    vec![DeregisterDenomResponse {}]
}

pub fn pool_creation_response_samples() -> Vec<PoolCreationResponse> {
    vec![
        PoolCreationResponse {
            vlp_contract: "cosmos1vlp".to_string(),
            tx_id: "tx-pool-create".to_string(),
            mint_lp_tokens: Uint256::MAX,
            sender: ccu("cosmos", "cosmos1sender"),
        },
        PoolCreationResponse {
            vlp_contract: String::new(),
            tx_id: String::new(),
            mint_lp_tokens: Uint256::zero(),
            sender: ccu("cosmos", ""),
        },
    ]
}

pub fn concentrated_pool_creation_response_samples() -> Vec<ConcentratedPoolCreationResponse> {
    pool_key_samples()
        .into_iter()
        .map(|pool_key| ConcentratedPoolCreationResponse {
            pool_key,
            vlp_contract: "cosmos1vlp".to_string(),
            tx_id: "tx-concentrated-pool-create".to_string(),
            sender: ccu("cosmos", "cosmos1sender"),
        })
        .collect()
}

pub fn add_liquidity_response_samples() -> Vec<AddLiquidityResponse> {
    vec![
        AddLiquidityResponse {
            mint_lp_tokens: Uint256::MAX,
            vlp_address: "cosmos1vlp".to_string(),
            tx_id: "tx-add-liquidity".to_string(),
            sender: ccu("cosmos", "cosmos1sender"),
        },
        AddLiquidityResponse {
            mint_lp_tokens: Uint256::zero(),
            vlp_address: String::new(),
            tx_id: String::new(),
            sender: ccu("cosmos", ""),
        },
    ]
}

pub fn concentrated_add_liquidity_response_samples() -> Vec<ConcentratedAddLiquidityResponse> {
    vec![
        ConcentratedAddLiquidityResponse {
            vlp_address: "cosmos1vlp".to_string(),
            tx_id: "tx-concentrated-add-liquidity".to_string(),
            sender: ccu("cosmos", "cosmos1sender"),
            position_id: Uint128::MAX,
            liquidity_delta: Uint128::new(42),
        },
        ConcentratedAddLiquidityResponse {
            vlp_address: String::new(),
            tx_id: String::new(),
            sender: ccu("cosmos", ""),
            position_id: Uint128::zero(),
            liquidity_delta: Uint128::zero(),
        },
    ]
}

pub fn remove_liquidity_response_samples() -> Vec<RemoveLiquidityResponse> {
    pair_with_amount_samples()
        .into_iter()
        .map(|liquidity_removed| RemoveLiquidityResponse {
            liquidity_removed,
            burn_lp_tokens: Uint256::MAX,
            vlp_address: "cosmos1vlp".to_string(),
        })
        .collect()
}

pub fn concentrated_remove_liquidity_response_samples() -> Vec<ConcentratedRemoveLiquidityResponse>
{
    let pool_key = pool_key_samples().into_iter().next().unwrap();
    let liquidity_removed = pair_with_amount_samples().into_iter().next().unwrap();
    vec![
        ConcentratedRemoveLiquidityResponse {
            pool_key: pool_key.clone(),
            position_id: Uint128::MAX,
            liquidity_removed: liquidity_removed.clone(),
            liquidity_delta: Uint128::new(7),
            liquidity_after: Uint128::zero(),
            vlp_address: "cosmos1vlp".to_string(),
            tx_id: "tx-concentrated-remove-liquidity".to_string(),
            sender: ccu("cosmos", "cosmos1sender"),
            position_burned: true,
        },
        ConcentratedRemoveLiquidityResponse {
            pool_key,
            position_id: Uint128::zero(),
            liquidity_removed,
            liquidity_delta: Uint128::MAX,
            liquidity_after: Uint128::MAX,
            vlp_address: "cosmos1vlp2".to_string(),
            tx_id: "tx-concentrated-remove-liquidity-2".to_string(),
            sender: ccu("evm", "0xsender"),
            position_burned: false,
        },
    ]
}

pub fn concentrated_collect_fees_response_samples() -> Vec<ConcentratedCollectFeesResponse> {
    let pool_key = pool_key_samples().into_iter().next().unwrap();
    vec![ConcentratedCollectFeesResponse {
        pool_key,
        position_id: Uint128::MAX,
        amount_0: Uint128::new(1),
        amount_1: Uint128::new(2),
        vlp_address: "cosmos1vlp".to_string(),
        tx_id: "tx-collect-fees".to_string(),
        sender: ccu("cosmos", "cosmos1sender"),
        recipient: ccu("cosmos", "cosmos1recipient"),
    }]
}

pub fn concentrated_collect_protocol_fees_response_samples(
) -> Vec<ConcentratedCollectProtocolFeesResponse> {
    let pool_key = pool_key_samples().into_iter().next().unwrap();
    vec![ConcentratedCollectProtocolFeesResponse {
        pool_key,
        amount_0: Uint128::MAX,
        amount_1: Uint128::zero(),
        vlp_address: "cosmos1vlp".to_string(),
        tx_id: "tx-collect-protocol-fees".to_string(),
        sender: ccu("cosmos", "cosmos1sender"),
        recipient: ccu("cosmos", "cosmos1recipient"),
    }]
}

pub fn swap_response_samples() -> Vec<SwapResponse> {
    vec![
        SwapResponse {
            amount_out: Uint256::MAX,
            tx_id: "tx-swap-1".to_string(),
        },
        SwapResponse {
            amount_out: Uint256::zero(),
            tx_id: "tx-swap-2".to_string(),
        },
    ]
}

pub fn transfer_voucher_response_samples() -> Vec<TransferVoucherResponse> {
    vec![
        TransferVoucherResponse {
            token: token("abc"),
            tx_id: "tx-transfer-voucher".to_string(),
        },
        // `Token` cannot itself be empty (`Token::validate` rejects it), so
        // the boundary sample uses the shortest legal token instead.
        TransferVoucherResponse {
            token: token("a"),
            tx_id: String::new(),
        },
    ]
}

pub fn deposit_token_response_samples() -> Vec<DepositTokenResponse> {
    vec![
        DepositTokenResponse {
            amount: Uint256::MAX,
            token: token("abc"),
            sender: ccu("cosmos", "cosmos1sender"),
        },
        // `Token` cannot itself be empty (`Token::validate` rejects it), so
        // the boundary sample uses the shortest legal token instead.
        DepositTokenResponse {
            amount: Uint256::zero(),
            token: token("a"),
            sender: ccu("cosmos", ""),
        },
    ]
}

/// The sentinel success ack payload (`Ok(b"1")`), modeled as `Vec<u8>`.
pub fn sentinel_samples() -> Vec<Vec<u8>> {
    vec![vec![b'1']]
}
