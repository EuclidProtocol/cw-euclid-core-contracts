//! Deep module: pure builder helpers that produce serialised
//! `RouterCrossChainExecuteMsg` payloads. No storage access, no submessages —
//! just typed inputs in, `Binary` (or a typed enum) out. The thin surface
//! makes table-driven tests easy and keeps the variant-specific knowledge in
//! one place.

use cosmwasm_std::{to_json_binary, Binary};
use euclid::{
    cross_chain_user::CrossChainUser, error::ContractError, msgs::vlp::base::PoolConfig,
    token::PairWithDenomAndAmount,
};
use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;

/// Builds a `RouterCrossChainExecuteMsg::RequestPoolCreation` packet ready to
/// be handed to main factory's `ProxySendPacket`.
pub fn request_pool_creation(
    sender: CrossChainUser,
    tx_id: String,
    pair: PairWithDenomAndAmount,
    pool_config: PoolConfig,
    slippage_tolerance_bps: u64,
) -> Result<Binary, ContractError> {
    let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
        sender,
        tx_id,
        pair,
        pool_config,
        slippage_tolerance_bps,
    };
    Ok(to_json_binary(&msg)?)
}

/// Builds a `RouterCrossChainExecuteMsg::AddLiquidity` packet for the CP/Stable
/// add-liquidity flow. The packet is dispatched through main factory's
/// `ProxySendPacket`.
pub fn add_liquidity(
    sender: CrossChainUser,
    tx_id: String,
    pair: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
) -> Result<Binary, ContractError> {
    let msg = RouterCrossChainExecuteMsg::AddLiquidity {
        sender,
        slippage_tolerance_bps,
        pair,
        tx_id,
    };
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
            let decoded: RouterCrossChainExecuteMsg = from_json(&bin).unwrap();
            match decoded {
                RouterCrossChainExecuteMsg::RequestPoolCreation {
                    tx_id: decoded_tx_id,
                    slippage_tolerance_bps,
                    ..
                } => {
                    assert_eq!(decoded_tx_id, *tx_id);
                    assert_eq!(slippage_tolerance_bps, *slippage);
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
            let decoded: RouterCrossChainExecuteMsg = from_json(&bin).unwrap();
            match decoded {
                RouterCrossChainExecuteMsg::AddLiquidity {
                    tx_id: decoded_tx_id,
                    slippage_tolerance_bps,
                    sender,
                    pair,
                } => {
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
