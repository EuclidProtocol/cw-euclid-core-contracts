//! Deep module: pure builder helpers that produce serialised
//! `RouterReceiveMsg` payloads. No storage access, no submessages —
//! just typed inputs in, `Binary` (or a typed enum) out. The thin surface
//! makes table-driven tests easy and keeps the variant-specific knowledge in
//! one place.

use cosmwasm_std::{to_json_binary, Binary, Uint256};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::vlp::base::{PoolConfig, PoolKey},
    token::{Pair, PairWithDenomAndAmount},
};
use euclid_ibc::wire::msgs::AddLiquiditySendMsg;
use euclid_ibc::wire::msgs::RequestPoolCreationSendMsg;
use euclid_ibc::wire::{
    envelope::router::RouterReceiveMsg,
    msgs::{RemoveLiquiditySendMsg, RequestConcentratedPoolCreationSendMsg},
};

/// Builds a `RouterReceiveMsg::RequestPoolCreation` packet ready to
/// be returned in `Response::data` as a `PoolFactoryReply::SendPacket` for
/// main factory's reply handler to dispatch.
pub fn request_pool_creation(
    sender: CrossChainUser,
    tx_id: String,
    pair: PairWithDenomAndAmount,
    pool_config: PoolConfig,
    slippage_tolerance_bps: u64,
) -> Result<Binary, ContractError> {
    let msg = RouterReceiveMsg::RequestPoolCreation(RequestPoolCreationSendMsg {
        sender,
        tx_id,
        pair,
        pool_config,
        slippage_tolerance_bps,
    });
    Ok(to_json_binary(&msg)?)
}

/// Builds a `RouterReceiveMsg::AddLiquidity` packet for the CP/Stable
/// add-liquidity flow. The packet is returned in `Response::data` as a
/// `PoolFactoryReply::SendPacket` and dispatched by main factory's reply
/// handler.
pub fn add_liquidity(
    sender: CrossChainUser,
    tx_id: String,
    pair: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
) -> Result<Binary, ContractError> {
    let msg = RouterReceiveMsg::AddLiquidity(AddLiquiditySendMsg {
        sender,
        slippage_tolerance_bps,
        pair,
        tx_id,
    });
    Ok(to_json_binary(&msg)?)
}

/// Builds a `RouterReceiveMsg::RequestConcentratedPoolCreation`
/// packet for the CLP pool creation flow. The packet is returned in
/// `Response::data` as a `PoolFactoryReply::SendPacket` and dispatched by main
/// factory's reply handler.
pub fn request_concentrated_pool_creation(
    sender: CrossChainUser,
    tx_id: String,
    pair: PairWithDenomAndAmount,
    pool_key: PoolKey,
    slippage_tolerance_bps: u64,
    initial_tick: Option<i64>,
) -> Result<Binary, ContractError> {
    let msg =
        RouterReceiveMsg::RequestConcentratedPoolCreation(RequestConcentratedPoolCreationSendMsg {
            sender,
            tx_id,
            pair,
            pool_key,
            slippage_tolerance_bps,
            initial_tick,
        });
    Ok(to_json_binary(&msg)?)
}

/// Builds a `RouterReceiveMsg::RemoveLiquidity` packet for the
/// CP/Stable remove-liquidity flow. The packet is returned in `Response::data`
/// as a `PoolFactoryReply::SendPacket` and dispatched by main factory's reply
/// handler.
pub fn remove_liquidity(
    sender: CrossChainUser,
    tx_id: String,
    pair: Pair,
    lp_allocation: Uint256,
    recipient: CrossChainUser,
) -> Result<Binary, ContractError> {
    let msg = RouterReceiveMsg::RemoveLiquidity(RemoveLiquiditySendMsg {
        sender,
        lp_allocation,
        pair,
        recipient,
        tx_id,
    });
    Ok(to_json_binary(&msg)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::from_json;
    use euclid::{
        chain::ChainUid,
        msgs::vlp::base::PoolConfig,
        token::{Token, TokenType, TokenWithDenomAndAmount},
    };
    use euclid_ibc::wire::msgs::AddLiquiditySendMsg;
    use euclid_ibc::wire::msgs::RequestPoolCreationSendMsg;

    fn sample_pair() -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
                amount: cosmwasm_std::Uint256::from(100u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                    decimals: None,
                },
                amount: cosmwasm_std::Uint256::from(100u128),
            },
        }
    }

    fn sample_sender() -> CrossChainUser {
        CrossChainUser {
            chain_uid: ChainUid::create("chainx".to_string()).unwrap(),
            address: "user1".to_string(),
        }
    }

    #[test]
    fn test_request_pool_creation_roundtrip() {
        let cases: &[(&str, u64)] = &[("tx_1", 50), ("tx_2", 1), ("tx_3", 10_000)];
        for (tx_id, slippage) in cases {
            let bin = request_pool_creation(
                sample_sender(),
                (*tx_id).to_string(),
                sample_pair(),
                PoolConfig::ConstantProduct {},
                *slippage,
            )
            .unwrap();
            let decoded: RouterReceiveMsg = from_json(&bin).unwrap();
            match decoded {
                RouterReceiveMsg::RequestPoolCreation(RequestPoolCreationSendMsg {
                    tx_id: decoded_tx_id,
                    slippage_tolerance_bps,
                    ..
                }) => {
                    assert_eq!(decoded_tx_id, *tx_id);
                    assert_eq!(slippage_tolerance_bps, *slippage);
                }
                _ => panic!("unexpected variant"),
            }
        }
    }

    #[test]
    fn test_remove_liquidity_roundtrip() {
        let recipient = CrossChainUser {
            chain_uid: ChainUid::create("chainx".to_string()).unwrap(),
            address: "recipient".to_string(),
        };
        let cases: &[(&str, u128)] = &[("tx_r1", 1), ("tx_r2", 100), ("tx_r3", 1_000_000_000)];
        let pair = sample_pair().get_pair().unwrap();
        for (tx_id, alloc) in cases {
            let bin = remove_liquidity(
                sample_sender(),
                (*tx_id).to_string(),
                pair.clone(),
                cosmwasm_std::Uint256::from(*alloc),
                recipient.clone(),
            )
            .unwrap();
            let decoded: RouterReceiveMsg = from_json(&bin).unwrap();
            match decoded {
                RouterReceiveMsg::RemoveLiquidity(inner) => {
                    assert_eq!(inner.tx_id, *tx_id);
                    assert_eq!(inner.lp_allocation, cosmwasm_std::Uint256::from(*alloc));
                    assert_eq!(inner.sender, sample_sender());
                    assert_eq!(inner.pair, pair);
                    assert_eq!(inner.recipient, recipient);
                }
                _ => panic!("unexpected variant"),
            }
        }
    }

    #[test]
    fn test_request_concentrated_pool_creation_roundtrip() {
        use euclid::msgs::vlp::base::{PoolKey, PoolType};
        let pair = sample_pair().get_pair().unwrap();
        let cases: &[(&str, u64, u64, u64, Option<i64>)] = &[
            ("tx_c1", 500, 10, 50, None),
            ("tx_c2", 3_000, 60, 100, Some(0)),
            ("tx_c3", 10_000, 200, 10_000, Some(-100)),
        ];
        for (tx_id, fee_tier_bps, tick_spacing, slippage, initial_tick) in cases {
            let pool_key = PoolKey {
                pair: pair.clone(),
                pool_type: PoolType::Concentrated {
                    fee_tier_bps: *fee_tier_bps,
                    tick_spacing: *tick_spacing,
                },
            };
            let bin = request_concentrated_pool_creation(
                sample_sender(),
                (*tx_id).to_string(),
                sample_pair(),
                pool_key.clone(),
                *slippage,
                *initial_tick,
            )
            .unwrap();
            let decoded: RouterReceiveMsg = from_json(&bin).unwrap();
            match decoded {
                RouterReceiveMsg::RequestConcentratedPoolCreation(inner) => {
                    assert_eq!(inner.tx_id, *tx_id);
                    assert_eq!(inner.slippage_tolerance_bps, *slippage);
                    assert_eq!(inner.initial_tick, *initial_tick);
                    assert_eq!(inner.pool_key, pool_key);
                    assert_eq!(inner.sender, sample_sender());
                }
                _ => panic!("unexpected variant"),
            }
        }
    }

    #[test]
    fn test_add_liquidity_roundtrip() {
        let cases: &[(&str, u64)] = &[("tx_a", 1), ("tx_b", 50), ("tx_c", 10_000)];
        for (tx_id, slippage) in cases {
            let bin = add_liquidity(
                sample_sender(),
                (*tx_id).to_string(),
                sample_pair(),
                *slippage,
            )
            .unwrap();
            let decoded: RouterReceiveMsg = from_json(&bin).unwrap();
            match decoded {
                RouterReceiveMsg::AddLiquidity(AddLiquiditySendMsg {
                    tx_id: decoded_tx_id,
                    slippage_tolerance_bps,
                    sender,
                    pair,
                }) => {
                    assert_eq!(decoded_tx_id, *tx_id);
                    assert_eq!(slippage_tolerance_bps, *slippage);
                    assert_eq!(sender, sample_sender());
                    assert_eq!(pair, sample_pair());
                }
                _ => panic!("unexpected variant"),
            }
        }
    }
}
