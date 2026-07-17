//! Boundary transcode module: the single place where the internal JSON domain
//! representation crosses to the wire leg encoding and back. Internal plumbing
//! stays JSON everywhere; `RouterReceiveMsg` / `FactoryReceiveMsg` / `*AckMsg` appear
//! only at the four boundaries (send emission, receive decode, ack emission,
//! ack decode). The wire mirrors serialize to byte-identical JSON, so the
//! `Json` arm of every transcode below is a passthrough.
//!
//! Ack payload types are selected exclusively by the originating send tag
//! through the tables below (there is no `IntoAck` coupling here): tag 13
//! (`TAG_SINGLE_SIDED_ADD_LIQUIDITY`) maps to `SingleSidedAddLiquidityAckMsg`
//! and tag 6 (`TAG_ADD_LIQUIDITY`) maps to `AddLiquidityAckMsg`.

use cosmwasm_std::Binary;
use euclid_encoding::{AbiDecode, AbiEncode, Encoding, EncodingError, JsonDecode, JsonEncode};

use crate::wire::envelope::AcknowledgementMsg;
use crate::wire::envelope::{factory, router};
use crate::wire::msgs::*;

/// Wire bytes to the received wire msg. Strict: decode failure is an error, no
/// fallback probing between encodings. Typed encode needs no helper; callers
/// holding a typed value use `euclid_encoding::encode(&msg, encoding)` directly.
pub fn decode_router_receive(
    bytes: &[u8],
    encoding: Encoding,
) -> Result<router::RouterReceiveMsg, EncodingError> {
    euclid_encoding::decode(bytes, encoding)
}

/// Wire bytes to the received wire msg. Strict: decode failure is an error, no
/// fallback probing between encodings.
pub fn decode_factory_receive(
    bytes: &[u8],
    encoding: Encoding,
) -> Result<factory::FactoryReceiveMsg, EncodingError> {
    euclid_encoding::decode(bytes, encoding)
}

fn ack_json_to_abi<S>(ack_json: &[u8]) -> Result<Binary, EncodingError>
where
    AcknowledgementMsg<S>: JsonDecode + AbiEncode,
{
    let ack = AcknowledgementMsg::<S>::from_json_bytes(ack_json)?;
    Ok(Binary::from(ack.to_abi_bytes()?))
}

fn ack_abi_to_json<S>(ack_wire: &[u8]) -> Result<Binary, EncodingError>
where
    AcknowledgementMsg<S>: AbiDecode + JsonEncode,
{
    let ack = AcknowledgementMsg::<S>::from_abi_bytes(ack_wire)?;
    Ok(Binary::from(ack.to_json_bytes()?))
}

/// Internal `AcknowledgementMsg<Response>` JSON to wire bytes. "router ack"
/// means the ack for a `RouterReceiveMsg` (produced by the hub, consumed by a
/// factory). Json arm is a byte passthrough; the Abi arm parses the JSON as
/// `AcknowledgementMsg<AckMsg>` (byte-identical serde mirror) keyed by `tag`
/// and re-encodes. The Error variant is handled uniformly for every tag.
pub fn router_ack_json_to_wire(
    tag: u8,
    ack_json: &[u8],
    encoding: Encoding,
) -> Result<Binary, EncodingError> {
    match encoding {
        Encoding::Json => Ok(Binary::from(ack_json.to_vec())),
        Encoding::Abi => match tag {
            router::TAG_REGISTER_DENOM => ack_json_to_abi::<RegisterDenomAckMsg>(ack_json),
            router::TAG_DEREGISTER_DENOM => ack_json_to_abi::<DeregisterDenomAckMsg>(ack_json),
            router::TAG_TRANSFER_VOUCHER => ack_json_to_abi::<TransferVoucherAckMsg>(ack_json),
            router::TAG_DEPOSIT_TOKEN => ack_json_to_abi::<DepositTokenAckMsg>(ack_json),
            // Regression (task-transcode-fix): pool creation acks carry the add
            // liquidity response, not a dedicated pool-creation response.
            // Creation chains into the initial liquidity add: the router's
            // `on_add_liquidity_reply` (reply.rs) sets the final reply data to
            // `AcknowledgementMsg<AddLiquidityResponse>` (classic) /
            // `<ConcentratedAddLiquidityResponse>` (concentrated), and the
            // factory decodes exactly those (`ack_and_timeout.rs`). So tags 4/5
            // map to the add liquidity ack mirrors, never a pool-creation one.
            router::TAG_REQUEST_POOL_CREATION => ack_json_to_abi::<AddLiquidityAckMsg>(ack_json),
            router::TAG_REQUEST_CONCENTRATED_POOL_CREATION => {
                ack_json_to_abi::<AddConcentratedLiquidityAckMsg>(ack_json)
            }
            router::TAG_ADD_LIQUIDITY => ack_json_to_abi::<AddLiquidityAckMsg>(ack_json),
            router::TAG_ADD_CONCENTRATED_LIQUIDITY => {
                ack_json_to_abi::<AddConcentratedLiquidityAckMsg>(ack_json)
            }
            router::TAG_REMOVE_LIQUIDITY => ack_json_to_abi::<RemoveLiquidityAckMsg>(ack_json),
            router::TAG_REMOVE_CONCENTRATED_LIQUIDITY => {
                ack_json_to_abi::<RemoveConcentratedLiquidityAckMsg>(ack_json)
            }
            router::TAG_COLLECT_CONCENTRATED_FEES => {
                ack_json_to_abi::<CollectConcentratedFeesAckMsg>(ack_json)
            }
            router::TAG_COLLECT_CONCENTRATED_PROTOCOL_FEES => {
                ack_json_to_abi::<CollectConcentratedProtocolFeesAckMsg>(ack_json)
            }
            router::TAG_SWAP => ack_json_to_abi::<SwapAckMsg>(ack_json),
            router::TAG_SINGLE_SIDED_ADD_LIQUIDITY => {
                ack_json_to_abi::<SingleSidedAddLiquidityAckMsg>(ack_json)
            }
            other => Err(EncodingError::UnknownDiscriminant {
                type_name: "RouterReceiveMsg",
                discriminant: other,
            }),
        },
    }
}

/// Wire ack bytes to internal `AcknowledgementMsg<Response>` JSON. The reverse
/// of `router_ack_json_to_wire`; same tag table over `ack_abi_to_json`.
pub fn router_ack_wire_to_json(
    tag: u8,
    ack_wire: &[u8],
    encoding: Encoding,
) -> Result<Binary, EncodingError> {
    match encoding {
        Encoding::Json => Ok(Binary::from(ack_wire.to_vec())),
        Encoding::Abi => match tag {
            router::TAG_REGISTER_DENOM => ack_abi_to_json::<RegisterDenomAckMsg>(ack_wire),
            router::TAG_DEREGISTER_DENOM => ack_abi_to_json::<DeregisterDenomAckMsg>(ack_wire),
            router::TAG_TRANSFER_VOUCHER => ack_abi_to_json::<TransferVoucherAckMsg>(ack_wire),
            router::TAG_DEPOSIT_TOKEN => ack_abi_to_json::<DepositTokenAckMsg>(ack_wire),
            // Regression (task-transcode-fix): pool creation acks carry the add
            // liquidity response, not a dedicated pool-creation response.
            // Creation chains into the initial liquidity add: the router's
            // `on_add_liquidity_reply` (reply.rs) sets the final reply data to
            // `AcknowledgementMsg<AddLiquidityResponse>` (classic) /
            // `<ConcentratedAddLiquidityResponse>` (concentrated), and the
            // factory decodes exactly those (`ack_and_timeout.rs`). So tags 4/5
            // map to the add liquidity ack mirrors, never a pool-creation one.
            router::TAG_REQUEST_POOL_CREATION => ack_abi_to_json::<AddLiquidityAckMsg>(ack_wire),
            router::TAG_REQUEST_CONCENTRATED_POOL_CREATION => {
                ack_abi_to_json::<AddConcentratedLiquidityAckMsg>(ack_wire)
            }
            router::TAG_ADD_LIQUIDITY => ack_abi_to_json::<AddLiquidityAckMsg>(ack_wire),
            router::TAG_ADD_CONCENTRATED_LIQUIDITY => {
                ack_abi_to_json::<AddConcentratedLiquidityAckMsg>(ack_wire)
            }
            router::TAG_REMOVE_LIQUIDITY => ack_abi_to_json::<RemoveLiquidityAckMsg>(ack_wire),
            router::TAG_REMOVE_CONCENTRATED_LIQUIDITY => {
                ack_abi_to_json::<RemoveConcentratedLiquidityAckMsg>(ack_wire)
            }
            router::TAG_COLLECT_CONCENTRATED_FEES => {
                ack_abi_to_json::<CollectConcentratedFeesAckMsg>(ack_wire)
            }
            router::TAG_COLLECT_CONCENTRATED_PROTOCOL_FEES => {
                ack_abi_to_json::<CollectConcentratedProtocolFeesAckMsg>(ack_wire)
            }
            router::TAG_SWAP => ack_abi_to_json::<SwapAckMsg>(ack_wire),
            router::TAG_SINGLE_SIDED_ADD_LIQUIDITY => {
                ack_abi_to_json::<SingleSidedAddLiquidityAckMsg>(ack_wire)
            }
            other => Err(EncodingError::UnknownDiscriminant {
                type_name: "RouterReceiveMsg",
                discriminant: other,
            }),
        },
    }
}

/// Internal `AcknowledgementMsg<Response>` JSON to wire bytes for a
/// `FactoryReceiveMsg` ack (produced by a factory, consumed by the hub). Json arm
/// is a byte passthrough; the Abi arm parses the JSON keyed by `tag`.
pub fn factory_ack_json_to_wire(
    tag: u8,
    ack_json: &[u8],
    encoding: Encoding,
) -> Result<Binary, EncodingError> {
    match encoding {
        Encoding::Json => Ok(Binary::from(ack_json.to_vec())),
        Encoding::Abi => match tag {
            factory::TAG_REGISTER_FACTORY => ack_json_to_abi::<RegisterFactoryAckMsg>(ack_json),
            factory::TAG_RELEASE_ESCROW => ack_json_to_abi::<ReleaseEscrowAckMsg>(ack_json),
            other => Err(EncodingError::UnknownDiscriminant {
                type_name: "FactoryReceiveMsg",
                discriminant: other,
            }),
        },
    }
}

/// Wire ack bytes to internal `AcknowledgementMsg<Response>` JSON for a
/// `FactoryReceiveMsg` ack. The reverse of `factory_ack_json_to_wire`.
pub fn factory_ack_wire_to_json(
    tag: u8,
    ack_wire: &[u8],
    encoding: Encoding,
) -> Result<Binary, EncodingError> {
    match encoding {
        Encoding::Json => Ok(Binary::from(ack_wire.to_vec())),
        Encoding::Abi => match tag {
            factory::TAG_REGISTER_FACTORY => ack_abi_to_json::<RegisterFactoryAckMsg>(ack_wire),
            factory::TAG_RELEASE_ESCROW => ack_abi_to_json::<ReleaseEscrowAckMsg>(ack_wire),
            other => Err(EncodingError::UnknownDiscriminant {
                type_name: "FactoryReceiveMsg",
                discriminant: other,
            }),
        },
    }
}

/// Thin wrapper for the send handlers, which hold the outbound msg as a JSON
/// `Binary` (produced by `to_msg`) rather than as a typed value. Naming is
/// receiver perspective: the router sends `FactoryReceiveMsg` values. Json arm
/// is a clone passthrough; the Abi arm parses the wire enum from JSON and
/// re-encodes to wire bytes in one step.
pub fn encode_factory_receive_from_json(
    json: &Binary,
    encoding: Encoding,
) -> Result<Binary, EncodingError> {
    match encoding {
        Encoding::Json => Ok(json.clone()),
        Encoding::Abi => {
            let msg = factory::FactoryReceiveMsg::from_json_bytes(json.as_slice())?;
            Ok(Binary::from(euclid_encoding::encode(&msg, Encoding::Abi)?))
        }
    }
}

/// Thin wrapper for the send handlers, which hold the outbound msg as a JSON
/// `Binary` (produced by `to_msg`) rather than as a typed value. Naming is
/// receiver perspective: a factory sends `RouterReceiveMsg` values. Json arm is
/// a clone passthrough; the Abi arm parses the wire enum from JSON and
/// re-encodes to wire bytes in one step.
pub fn encode_router_receive_from_json(
    json: &Binary,
    encoding: Encoding,
) -> Result<Binary, EncodingError> {
    match encoding {
        Encoding::Json => Ok(json.clone()),
        Encoding::Abi => {
            let msg = router::RouterReceiveMsg::from_json_bytes(json.as_slice())?;
            Ok(Binary::from(euclid_encoding::encode(&msg, Encoding::Abi)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{to_json_binary, Uint128, Uint256};
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::router::execute::{RegisterFactoryChainEvm, RegisterFactoryChainType};
    use euclid::msgs::vlp::base::{PoolConfig, PoolKey, PoolType};
    use euclid::recipient::Recipient;
    use euclid::swap::NextSwapPair;
    use euclid::token::{
        Pair, PairWithAmount, PairWithDenomAndAmount, Token, TokenType, TokenWithAmount,
        TokenWithDenom, TokenWithDenomAndAmount,
    };

    use crate::wire::envelope::{factory::FactoryReceiveMsg, router::RouterReceiveMsg};

    // ----- shared fixture builders -----

    fn chain(uid: &str) -> ChainUid {
        ChainUid::create(uid.to_string()).unwrap()
    }

    fn ccu(chain_uid: &str, addr: &str) -> CrossChainUser {
        CrossChainUser::new(chain(chain_uid), addr.to_string())
    }

    fn token(denom: &str) -> Token {
        Token::create(denom.to_string()).unwrap()
    }

    fn native(denom: &str) -> TokenType {
        TokenType::Native {
            denom: denom.to_string(),
            decimals: None,
        }
    }

    fn token_with_denom(denom: &str, native_denom: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: token(denom),
            token_type: native(native_denom),
        }
    }

    fn pda() -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token("abc"),
                amount: Uint256::from(1_000u128),
                token_type: native("uabc"),
            },
            token_2: TokenWithDenomAndAmount {
                token: token("xyz"),
                amount: Uint256::from(2_000u128),
                token_type: native("uxyz"),
            },
        }
    }

    fn pool_key() -> PoolKey {
        PoolKey {
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 30,
                tick_spacing: 60,
            },
        }
    }

    fn recipient() -> Recipient {
        Recipient {
            recipient: ccu("chain2", "recipient-addr"),
            amount: Limit::LessThanOrEqual(Uint256::from(1_000u128)),
            denom: TokenType::Voucher {},
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }
    }

    fn next_swap() -> NextSwapPair {
        NextSwapPair {
            token_in: token("abc"),
            token_out: token("xyz"),
            pool_key: None,
            test_fail: None,
        }
    }

    fn liquidity_removed() -> PairWithAmount {
        PairWithAmount::new(
            TokenWithAmount {
                token: token("abc"),
                amount: Uint256::zero(),
            },
            TokenWithAmount {
                token: token("def"),
                amount: Uint256::MAX,
            },
        )
        .unwrap()
    }

    // ----- wire send fixtures (one per router variant) -----

    fn register_denom() -> RouterReceiveMsg {
        RouterReceiveMsg::RegisterDenom(RegisterDenomSendMsg {
            sender: ccu("chain1", "a"),
            tx_id: "t0".to_string(),
            token: token_with_denom("abc", "uabc"),
        })
    }

    fn deregister_denom() -> RouterReceiveMsg {
        RouterReceiveMsg::DeregisterDenom(DeregisterDenomSendMsg {
            sender: ccu("chain1", "a"),
            tx_id: "t1".to_string(),
            token: token_with_denom("abc", "uabc"),
        })
    }

    fn transfer_voucher() -> RouterReceiveMsg {
        RouterReceiveMsg::TransferVoucher(TransferVoucherSendMsg {
            sender: ccu("chain1", "a"),
            token: token("abc"),
            amount: Uint256::from(10u128),
            from: None,
            recipients: vec![recipient()],
            tx_id: "t2".to_string(),
        })
    }

    fn deposit_token() -> RouterReceiveMsg {
        RouterReceiveMsg::DepositToken(DepositTokenSendMsg {
            sender: ccu("chain1", "a"),
            asset_in: token_with_denom("abc", "uabc"),
            amount_in: Uint256::from(123u128),
            recipients: vec![recipient()],
            tx_id: "t3".to_string(),
        })
    }

    fn request_pool_creation() -> RouterReceiveMsg {
        RouterReceiveMsg::RequestPoolCreation(RequestPoolCreationSendMsg {
            sender: ccu("chain1", "a"),
            tx_id: "t4".to_string(),
            pair: pda(),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 100,
        })
    }

    fn request_concentrated_pool_creation() -> RouterReceiveMsg {
        RouterReceiveMsg::RequestConcentratedPoolCreation(RequestConcentratedPoolCreationSendMsg {
            sender: ccu("chain1", "a"),
            tx_id: "t5".to_string(),
            pair: pda(),
            pool_key: pool_key(),
            slippage_tolerance_bps: 100,
            initial_tick: None,
        })
    }

    fn add_liquidity() -> RouterReceiveMsg {
        RouterReceiveMsg::AddLiquidity(AddLiquiditySendMsg {
            sender: ccu("chain1", "factory-addr"),
            slippage_tolerance_bps: 100,
            pair: pda(),
            tx_id: "t6".to_string(),
        })
    }

    fn add_concentrated_liquidity() -> RouterReceiveMsg {
        RouterReceiveMsg::AddConcentratedLiquidity(AddConcentratedLiquiditySendMsg {
            sender: ccu("chain1", "a"),
            pair: pda(),
            pool_key: pool_key(),
            lower_tick_index: -60,
            upper_tick_index: 60,
            position_id: Some(Uint128::new(7)),
            slippage_tolerance_bps: 100,
            tx_id: "t7".to_string(),
        })
    }

    fn remove_liquidity() -> RouterReceiveMsg {
        RouterReceiveMsg::RemoveLiquidity(RemoveLiquiditySendMsg {
            sender: ccu("chain1", "a"),
            lp_allocation: Uint256::MAX,
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            recipient: ccu("chain2", "recipient-addr"),
            tx_id: "t8".to_string(),
        })
    }

    fn remove_concentrated_liquidity() -> RouterReceiveMsg {
        RouterReceiveMsg::RemoveConcentratedLiquidity(RemoveConcentratedLiquiditySendMsg {
            sender: ccu("chain1", "a"),
            pool_key: pool_key(),
            position_id: Uint128::new(9),
            liquidity_delta: Uint128::MAX,
            recipient: ccu("chain2", "recipient-addr"),
            tx_id: "t9".to_string(),
        })
    }

    fn collect_concentrated_fees() -> RouterReceiveMsg {
        RouterReceiveMsg::CollectConcentratedFees(CollectConcentratedFeesSendMsg {
            sender: ccu("chain1", "a"),
            pool_key: pool_key(),
            position_id: Uint128::new(3),
            recipient: ccu("chain2", "recipient-addr"),
            tx_id: "t10".to_string(),
        })
    }

    fn collect_concentrated_protocol_fees() -> RouterReceiveMsg {
        RouterReceiveMsg::CollectConcentratedProtocolFees(CollectConcentratedProtocolFeesSendMsg {
            sender: ccu("chain1", "a"),
            pool_key: pool_key(),
            recipient: ccu("chain2", "recipient-addr"),
            amount_0_requested: Uint128::zero(),
            amount_1_requested: Uint128::MAX,
            tx_id: "t11".to_string(),
        })
    }

    fn swap() -> RouterReceiveMsg {
        RouterReceiveMsg::Swap(SwapSendMsg {
            sender: ccu("chain1", "factory-addr"),
            asset_in: token_with_denom("abc", "uabc"),
            amount_in: Uint256::from(1_000u128),
            asset_out: token("out"),
            min_amount_out: Uint256::from(1u128),
            swaps: vec![next_swap()],
            recipients: vec![recipient()],
            partner_fee_amount: Uint256::from(10u128),
            partner_fee_recipient: ccu("chain1", "partner-addr"),
            tx_id: "t12".to_string(),
        })
    }

    fn single_sided_add_liquidity() -> RouterReceiveMsg {
        RouterReceiveMsg::SingleSidedAddLiquidity(SingleSidedAddLiquiditySendMsg {
            sender: ccu("chain1", "factory-addr"),
            asset_in: token_with_denom("abc", "uabc"),
            amount_in: Uint256::from(500u128),
            swap_amount: Uint256::from(250u128),
            pair: Pair::new(token("abc"), token("def")).unwrap(),
            swaps: vec![NextSwapPair {
                token_in: token("abc"),
                token_out: token("def"),
                pool_key: None,
                test_fail: None,
            }],
            min_lp_out: Uint256::from(1u128),
            partner_fee_amount: Uint256::zero(),
            partner_fee_recipient: ccu("chain1", "partner-addr"),
            tx_id: "t13".to_string(),
        })
    }

    // ----- wire send fixtures (factory) -----

    fn register_factory() -> FactoryReceiveMsg {
        FactoryReceiveMsg::RegisterFactory(RegisterFactorySendMsg {
            chain_uid: chain("chain1"),
            chain_type: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                factory_address: "0xabc".to_string(),
                factory_chain_id: "1".to_string(),
            }),
            tx_id: "f0".to_string(),
        })
    }

    fn release_escrow() -> FactoryReceiveMsg {
        FactoryReceiveMsg::ReleaseEscrow(ReleaseEscrowSendMsg {
            sender: ccu("chain1", "factory-addr"),
            token: token("abc"),
            recipient: "recipient-addr".to_string(),
            amount: Uint256::from(1_000u128),
            denom: native("uabc"),
            forwarding_message: None,
            tx_id: "f1".to_string(),
        })
    }

    // ----- send transcode tests -----

    #[test]
    fn json_send_encode_is_passthrough() {
        // The Json arm of the from-json wrapper is a passthrough over the
        // serialized wire enum JSON.
        let msg = add_liquidity();
        let json = to_json_binary(&msg).unwrap();
        assert_eq!(
            encode_router_receive_from_json(&json, Encoding::Json).unwrap(),
            json
        );
    }

    #[test]
    fn abi_send_roundtrips_through_decode() {
        let wire = swap();
        let bytes = Binary::from(euclid_encoding::encode(&wire, Encoding::Abi).unwrap());
        assert_eq!(decode_router_receive(&bytes, Encoding::Abi).unwrap(), wire);
    }

    #[test]
    fn abi_factory_send_roundtrips_through_decode() {
        let wire = release_escrow();
        let bytes = Binary::from(euclid_encoding::encode(&wire, Encoding::Abi).unwrap());
        assert_eq!(decode_factory_receive(&bytes, Encoding::Abi).unwrap(), wire);
    }

    #[test]
    fn json_send_from_json_is_passthrough_both_legs() {
        let wire = register_factory();
        let json = to_json_binary(&wire).unwrap();
        assert_eq!(
            encode_factory_receive_from_json(&json, Encoding::Json).unwrap(),
            json
        );
        // Abi arm parses then re-encodes; decoding it back yields the wire msg.
        let bytes = encode_factory_receive_from_json(&json, Encoding::Abi).unwrap();
        assert_eq!(decode_factory_receive(&bytes, Encoding::Abi).unwrap(), wire);
    }

    #[test]
    fn router_wire_tags_match_frozen_envelope() {
        assert_eq!(register_denom().wire_tag(), 0);
        assert_eq!(deregister_denom().wire_tag(), 1);
        assert_eq!(transfer_voucher().wire_tag(), 2);
        assert_eq!(deposit_token().wire_tag(), 3);
        assert_eq!(request_pool_creation().wire_tag(), 4);
        assert_eq!(request_concentrated_pool_creation().wire_tag(), 5);
        assert_eq!(add_liquidity().wire_tag(), 6);
        assert_eq!(add_concentrated_liquidity().wire_tag(), 7);
        assert_eq!(remove_liquidity().wire_tag(), 8);
        assert_eq!(remove_concentrated_liquidity().wire_tag(), 9);
        assert_eq!(collect_concentrated_fees().wire_tag(), 10);
        assert_eq!(collect_concentrated_protocol_fees().wire_tag(), 11);
        assert_eq!(swap().wire_tag(), 12);
        assert_eq!(single_sided_add_liquidity().wire_tag(), 13);
    }

    #[test]
    fn factory_wire_tags_match_frozen_envelope() {
        assert_eq!(register_factory().wire_tag(), 0);
        assert_eq!(release_escrow().wire_tag(), 1);
    }

    // ----- ack transcode tests -----

    #[test]
    fn json_ack_transcode_is_passthrough() {
        let ack = crate::wire::envelope::make_ack_fail("boom".to_string()).unwrap();
        assert_eq!(
            router_ack_json_to_wire(6, &ack, Encoding::Json).unwrap(),
            ack
        );
        assert_eq!(
            router_ack_wire_to_json(6, &ack, Encoding::Json).unwrap(),
            ack
        );
    }

    // Helper: build the internal `AcknowledgementMsg<AckMsg>` Ok-arm JSON, run
    // it through json_to_wire (Abi) then wire_to_json (Abi), and assert the
    // final JSON is byte-identical to the original.
    fn assert_router_ack_roundtrip<S>(tag: u8, ack: S)
    where
        S: serde::Serialize,
        AcknowledgementMsg<S>: JsonDecode + AbiDecode + AbiEncode + JsonEncode,
    {
        let json = to_json_binary(&AcknowledgementMsg::<S>::Ok(ack)).unwrap();
        let wire = router_ack_json_to_wire(tag, &json, Encoding::Abi).unwrap();
        let back = router_ack_wire_to_json(tag, &wire, Encoding::Abi).unwrap();
        assert_eq!(back, json);
    }

    fn assert_factory_ack_roundtrip<S>(tag: u8, ack: S)
    where
        S: serde::Serialize,
        AcknowledgementMsg<S>: JsonDecode + AbiDecode + AbiEncode + JsonEncode,
    {
        let json = to_json_binary(&AcknowledgementMsg::<S>::Ok(ack)).unwrap();
        let wire = factory_ack_json_to_wire(tag, &json, Encoding::Abi).unwrap();
        let back = factory_ack_wire_to_json(tag, &wire, Encoding::Abi).unwrap();
        assert_eq!(back, json);
    }

    #[test]
    fn abi_ack_roundtrips_per_tag() {
        // Empty acks: byte-identical `{"ok":{}}` roundtrip.
        assert_router_ack_roundtrip(0, RegisterDenomAckMsg {});
        assert_router_ack_roundtrip(1, DeregisterDenomAckMsg {});

        assert_router_ack_roundtrip(
            2,
            TransferVoucherAckMsg {
                token: token("abc"),
                tx_id: "tx-transfer".to_string(),
            },
        );
        assert_router_ack_roundtrip(
            3,
            DepositTokenAckMsg {
                amount: Uint256::MAX,
                token: token("abc"),
                sender: ccu("chain1", "sender-addr"),
            },
        );
        // Pool creation acks carry the add liquidity response (creation chains
        // into the initial add), so tags 4/5 use the add liquidity ack mirrors.
        assert_router_ack_roundtrip(
            4,
            AddLiquidityAckMsg {
                mint_lp_tokens: Uint256::MAX,
                vlp_address: "cosmos1vlp".to_string(),
                tx_id: "tx-request-pool".to_string(),
                sender: ccu("chain1", "sender-addr"),
            },
        );
        assert_router_ack_roundtrip(
            5,
            AddConcentratedLiquidityAckMsg {
                vlp_address: "cosmos1vlp".to_string(),
                tx_id: "tx-request-clp".to_string(),
                sender: ccu("chain1", "sender-addr"),
                position_id: Uint128::MAX,
                liquidity_delta: Uint128::new(42),
            },
        );
        assert_router_ack_roundtrip(
            6,
            AddLiquidityAckMsg {
                mint_lp_tokens: Uint256::MAX,
                vlp_address: "cosmos1vlp".to_string(),
                tx_id: "tx-add-liq".to_string(),
                sender: ccu("chain1", "sender-addr"),
            },
        );
        assert_router_ack_roundtrip(
            7,
            AddConcentratedLiquidityAckMsg {
                vlp_address: "cosmos1vlp".to_string(),
                tx_id: "tx-add-clp".to_string(),
                sender: ccu("chain1", "sender-addr"),
                position_id: Uint128::MAX,
                liquidity_delta: Uint128::new(42),
            },
        );
        assert_router_ack_roundtrip(
            8,
            RemoveLiquidityAckMsg {
                liquidity_removed: liquidity_removed(),
                burn_lp_tokens: Uint256::MAX,
                vlp_address: "cosmos1vlp".to_string(),
            },
        );
        assert_router_ack_roundtrip(
            9,
            RemoveConcentratedLiquidityAckMsg {
                pool_key: pool_key(),
                position_id: Uint128::MAX,
                liquidity_removed: liquidity_removed(),
                liquidity_delta: Uint128::new(7),
                liquidity_after: Uint128::zero(),
                vlp_address: "cosmos1vlp".to_string(),
                tx_id: "tx-remove-clp".to_string(),
                sender: ccu("chain1", "sender-addr"),
                position_burned: true,
            },
        );
        assert_router_ack_roundtrip(
            10,
            CollectConcentratedFeesAckMsg {
                pool_key: pool_key(),
                position_id: Uint128::MAX,
                amount_0: Uint128::new(1),
                amount_1: Uint128::new(2),
                vlp_address: "cosmos1vlp".to_string(),
                tx_id: "tx-collect-fees".to_string(),
                sender: ccu("chain1", "sender-addr"),
                recipient: ccu("chain1", "recipient-addr"),
            },
        );
        assert_router_ack_roundtrip(
            11,
            CollectConcentratedProtocolFeesAckMsg {
                pool_key: pool_key(),
                amount_0: Uint128::MAX,
                amount_1: Uint128::zero(),
                vlp_address: "cosmos1vlp".to_string(),
                tx_id: "tx-collect-protocol-fees".to_string(),
                sender: ccu("chain1", "sender-addr"),
                recipient: ccu("chain1", "recipient-addr"),
            },
        );
        assert_router_ack_roundtrip(
            12,
            SwapAckMsg {
                amount_out: Uint256::MAX,
                tx_id: "tx-swap".to_string(),
            },
        );
        assert_router_ack_roundtrip(
            13,
            SingleSidedAddLiquidityAckMsg {
                mint_lp_tokens: Uint256::from(1_000u128),
                vlp_address: "vlp1".to_string(),
                tx_id: "tx-1".to_string(),
                sender: ccu("chain1", "sender-addr"),
            },
        );

        // Factory acks.
        assert_factory_ack_roundtrip(
            0,
            RegisterFactoryAckMsg {
                factory_address: "cosmos1factory".to_string(),
                chain_id: "cosmoshub-4".to_string(),
            },
        );
        assert_factory_ack_roundtrip(
            1,
            ReleaseEscrowAckMsg {
                amount: Uint256::MAX,
                to_address: "cosmos1recipient".to_string(),
                escrow_balance: Uint256::zero(),
            },
        );
    }

    #[test]
    fn abi_error_ack_is_tag_independent() {
        let ack = crate::wire::envelope::make_ack_fail("boom".to_string()).unwrap();
        let w6 = router_ack_json_to_wire(6, &ack, Encoding::Abi).unwrap();
        let w13 = router_ack_json_to_wire(13, &ack, Encoding::Abi).unwrap();
        assert_eq!(w6, w13);
        assert_eq!(router_ack_wire_to_json(6, &w6, Encoding::Abi).unwrap(), ack);
    }

    #[test]
    fn unknown_tag_is_error() {
        let ack = crate::wire::envelope::make_ack_fail("boom".to_string()).unwrap();
        assert!(router_ack_json_to_wire(14, &ack, Encoding::Abi).is_err());
        assert!(factory_ack_json_to_wire(2, &ack, Encoding::Abi).is_err());
        assert!(router_ack_wire_to_json(14, &ack, Encoding::Abi).is_err());
        assert!(factory_ack_wire_to_json(2, &ack, Encoding::Abi).is_err());
    }
}
