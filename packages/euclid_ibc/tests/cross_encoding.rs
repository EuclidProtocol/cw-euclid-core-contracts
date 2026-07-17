//! Cross-encoding sanity (plan §9.4), table-driven over every message family's
//! sample set. For every sample: JSON-decode(JSON-encode(v)) and
//! ABI-decode(ABI-encode(v)) must be equal to `v`, and the two encodings must
//! actually produce different bytes. The distinctness assertion is the point
//! of this file: it guards against an `AbiEncode`/`JsonEncode` (or the shared
//! types' `*_to_sol`) impl that accidentally delegates to the wrong path and
//! would otherwise pass every roundtrip test while silently not exercising ABI
//! (or JSON) at all.
//!
//! Message families ride their wire mirrors (`RouterReceiveMsg`, `FactoryReceiveMsg`,
//! `AcknowledgementMsg<AckMsg>`); the shared types ride the `wire::types`
//! free functions, since the domain types no longer implement `AbiMap`.

mod common;

use std::fmt::Debug;

use alloy_sol_types::SolType;
use euclid_encoding::{decode, encode, AbiDecode, AbiEncode, Encoding, JsonDecode, JsonEncode};
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::envelope::router::RouterReceiveMsg;
use euclid_ibc::wire::envelope::AcknowledgementMsg;
use euclid_ibc::wire::msgs::{
    AddConcentratedLiquidityAckMsg, AddLiquidityAckMsg, CollectConcentratedFeesAckMsg,
    CollectConcentratedProtocolFeesAckMsg, DepositTokenAckMsg, DeregisterDenomAckMsg,
    RegisterDenomAckMsg, RegisterFactoryAckMsg, ReleaseEscrowAckMsg,
    RemoveConcentratedLiquidityAckMsg, RemoveLiquidityAckMsg, SwapAckMsg, TransferVoucherAckMsg,
};

/// JSON and ABI must decode to equal values, and the two on-wire encodings
/// must differ. For the wire mirror message families that carry a full
/// `AbiEncode`/`AbiDecode` pair.
fn assert_cross_encoding<T>(label: &str, value: &T)
where
    T: JsonEncode + JsonDecode + AbiEncode + AbiDecode + PartialEq + Debug,
{
    let json_bytes = encode(value, Encoding::Json)
        .unwrap_or_else(|e| panic!("json encode failed for {label}: {e}"));
    let abi_bytes = encode(value, Encoding::Abi)
        .unwrap_or_else(|e| panic!("abi encode failed for {label}: {e}"));

    let json_decoded: T = decode(&json_bytes, Encoding::Json)
        .unwrap_or_else(|e| panic!("json decode failed for {label}: {e}"));
    let abi_decoded: T = decode(&abi_bytes, Encoding::Abi)
        .unwrap_or_else(|e| panic!("abi decode failed for {label}: {e}"));

    assert_eq!(json_decoded, *value, "json roundtrip differs for {label}");
    assert_eq!(abi_decoded, *value, "abi roundtrip differs for {label}");
    assert_ne!(
        json_bytes, abi_bytes,
        "json and abi wire bytes must differ for {label}"
    );
}

/// The shared-type equivalent: JSON via the domain serde, ABI via the
/// `wire::types` free functions and the type's `Sol` alias.
macro_rules! assert_shared_cross_encoding {
    ($label:expr, $value:expr, $dty:ty, $sol:ty, $to:path, $from:path) => {{
        let value: $dty = $value;
        let json = value.to_json_bytes().expect("json encode");
        let json_back = <$dty as JsonDecode>::from_json_bytes(&json).expect("json decode");
        assert_eq!(json_back, value, "json roundtrip differs for {}", $label);

        let sol = $to(&value).expect("to_sol");
        let abi = <$sol as SolType>::abi_encode_params(&sol);
        let abi_sol = <$sol as SolType>::abi_decode_params(&abi).expect("abi decode");
        let abi_back = $from(abi_sol).expect("from_sol");
        assert_eq!(abi_back, value, "abi roundtrip differs for {}", $label);

        assert_ne!(
            json, abi,
            "json and abi wire bytes must differ for {}",
            $label
        );
    }};
}

#[test]
fn router_family_cross_encoding() {
    for (label, msg) in common::router_samples::all_samples() {
        assert_cross_encoding::<RouterReceiveMsg>(label, &msg);
    }
}

#[test]
fn factory_family_cross_encoding() {
    for msg in common::factory_samples::all_samples() {
        assert_cross_encoding::<FactoryReceiveMsg>("factory-sample", &msg);
    }
}

#[test]
fn shared_types_cross_encoding() {
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::limit::Limit;
    use euclid::msgs::router::execute::RegisterFactoryChainType;
    use euclid::msgs::vlp::base::{PoolConfig, PoolKey, PoolType};
    use euclid::recipient::Recipient;
    use euclid::swap::NextSwapPair;
    use euclid::token::{
        Pair, PairWithAmount, PairWithDenomAndAmount, Token, TokenType, TokenWithAmount,
        TokenWithDenom, TokenWithDenomAndAmount,
    };
    use euclid_ibc::wire::types::chain_uid::{chain_uid_from_sol, chain_uid_to_sol, ChainUidSol};
    use euclid_ibc::wire::types::cross_chain_user::{
        cross_chain_user_from_sol, cross_chain_user_to_sol, CrossChainUserSol,
    };
    use euclid_ibc::wire::types::limit::{limit_from_sol, limit_to_sol, LimitSol};
    use euclid_ibc::wire::types::next_swap_pair::{
        next_swap_pair_from_sol, next_swap_pair_to_sol, NextSwapPairSol,
    };
    use euclid_ibc::wire::types::pair::{pair_from_sol, pair_to_sol, PairSol};
    use euclid_ibc::wire::types::pair_with_amount::{
        pair_with_amount_from_sol, pair_with_amount_to_sol, PairWithAmountSol,
    };
    use euclid_ibc::wire::types::pair_with_denom_and_amount::{
        pair_with_denom_and_amount_from_sol, pair_with_denom_and_amount_to_sol,
        PairWithDenomAndAmountSol,
    };
    use euclid_ibc::wire::types::pool_config::{
        pool_config_from_sol, pool_config_to_sol, PoolConfigSol,
    };
    use euclid_ibc::wire::types::pool_key::{pool_key_from_sol, pool_key_to_sol, PoolKeySol};
    use euclid_ibc::wire::types::pool_type::{pool_type_from_sol, pool_type_to_sol, PoolTypeSol};
    use euclid_ibc::wire::types::recipient::{recipient_from_sol, recipient_to_sol, RecipientSol};
    use euclid_ibc::wire::types::register_factory_chain::{
        register_factory_chain_type_from_sol, register_factory_chain_type_to_sol,
        RegisterFactoryChainTypeSol,
    };
    use euclid_ibc::wire::types::token::{token_from_sol, token_to_sol, TokenSol};
    use euclid_ibc::wire::types::token_type::{
        token_type_from_sol, token_type_to_sol, TokenTypeSol,
    };
    use euclid_ibc::wire::types::token_with_amount::{
        token_with_amount_from_sol, token_with_amount_to_sol, TokenWithAmountSol,
    };
    use euclid_ibc::wire::types::token_with_denom::{
        token_with_denom_from_sol, token_with_denom_to_sol, TokenWithDenomSol,
    };
    use euclid_ibc::wire::types::token_with_denom_and_amount::{
        token_with_denom_and_amount_from_sol, token_with_denom_and_amount_to_sol,
        TokenWithDenomAndAmountSol,
    };

    for v in common::types_samples::token_samples() {
        assert_shared_cross_encoding!("Token", v, Token, TokenSol, token_to_sol, token_from_sol);
    }
    for v in common::types_samples::cross_chain_user_samples() {
        assert_shared_cross_encoding!(
            "CrossChainUser",
            v,
            CrossChainUser,
            CrossChainUserSol,
            cross_chain_user_to_sol,
            cross_chain_user_from_sol
        );
    }
    for v in common::types_samples::token_type_samples() {
        assert_shared_cross_encoding!(
            "TokenType",
            v,
            TokenType,
            TokenTypeSol,
            token_type_to_sol,
            token_type_from_sol
        );
    }
    for v in common::types_samples::limit_samples() {
        assert_shared_cross_encoding!("Limit", v, Limit, LimitSol, limit_to_sol, limit_from_sol);
    }
    for v in common::types_samples::recipient_samples() {
        assert_shared_cross_encoding!(
            "Recipient",
            v,
            Recipient,
            RecipientSol,
            recipient_to_sol,
            recipient_from_sol
        );
    }
    for v in common::types_samples::next_swap_pair_samples() {
        assert_shared_cross_encoding!(
            "NextSwapPair",
            v,
            NextSwapPair,
            NextSwapPairSol,
            next_swap_pair_to_sol,
            next_swap_pair_from_sol
        );
    }
    for v in common::types_samples::pool_key_samples() {
        assert_shared_cross_encoding!(
            "PoolKey",
            v,
            PoolKey,
            PoolKeySol,
            pool_key_to_sol,
            pool_key_from_sol
        );
    }
    for v in common::types_samples::pool_config_samples() {
        assert_shared_cross_encoding!(
            "PoolConfig",
            v,
            PoolConfig,
            PoolConfigSol,
            pool_config_to_sol,
            pool_config_from_sol
        );
    }
    for v in common::types_samples::register_factory_chain_samples() {
        assert_shared_cross_encoding!(
            "RegisterFactoryChainType",
            v,
            RegisterFactoryChainType,
            RegisterFactoryChainTypeSol,
            register_factory_chain_type_to_sol,
            register_factory_chain_type_from_sol
        );
    }
    for v in common::types_samples::chain_uid_samples() {
        assert_shared_cross_encoding!(
            "ChainUid",
            v,
            ChainUid,
            ChainUidSol,
            chain_uid_to_sol,
            chain_uid_from_sol
        );
    }
    for v in common::types_samples::pair_samples() {
        assert_shared_cross_encoding!("Pair", v, Pair, PairSol, pair_to_sol, pair_from_sol);
    }
    for v in common::types_samples::pair_with_amount_samples() {
        assert_shared_cross_encoding!(
            "PairWithAmount",
            v,
            PairWithAmount,
            PairWithAmountSol,
            pair_with_amount_to_sol,
            pair_with_amount_from_sol
        );
    }
    for v in common::types_samples::pair_with_denom_and_amount_samples() {
        assert_shared_cross_encoding!(
            "PairWithDenomAndAmount",
            v,
            PairWithDenomAndAmount,
            PairWithDenomAndAmountSol,
            pair_with_denom_and_amount_to_sol,
            pair_with_denom_and_amount_from_sol
        );
    }
    for v in common::types_samples::pool_type_samples() {
        assert_shared_cross_encoding!(
            "PoolType",
            v,
            PoolType,
            PoolTypeSol,
            pool_type_to_sol,
            pool_type_from_sol
        );
    }
    for v in common::types_samples::token_with_amount_samples() {
        assert_shared_cross_encoding!(
            "TokenWithAmount",
            v,
            TokenWithAmount,
            TokenWithAmountSol,
            token_with_amount_to_sol,
            token_with_amount_from_sol
        );
    }
    for v in common::types_samples::token_with_denom_samples() {
        assert_shared_cross_encoding!(
            "TokenWithDenom",
            v,
            TokenWithDenom,
            TokenWithDenomSol,
            token_with_denom_to_sol,
            token_with_denom_from_sol
        );
    }
    for v in common::types_samples::token_with_denom_and_amount_samples() {
        assert_shared_cross_encoding!(
            "TokenWithDenomAndAmount",
            v,
            TokenWithDenomAndAmount,
            TokenWithDenomAndAmountSol,
            token_with_denom_and_amount_to_sol,
            token_with_denom_and_amount_from_sol
        );
    }
}

#[test]
fn ack_family_cross_encoding() {
    for sample in common::ack_samples::register_factory_response_samples() {
        let ok: AcknowledgementMsg<RegisterFactoryAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<RegisterFactoryAckMsg>::Ok", &ok);
    }

    let error: AcknowledgementMsg<RegisterFactoryAckMsg> =
        AcknowledgementMsg::Error("boom".to_string());
    assert_cross_encoding("AcknowledgementMsg<RegisterFactoryAckMsg>::Error", &error);

    for sample in common::ack_samples::release_escrow_response_samples() {
        let ok: AcknowledgementMsg<ReleaseEscrowAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<ReleaseEscrowAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::register_denom_response_samples() {
        let ok: AcknowledgementMsg<RegisterDenomAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<RegisterDenomAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::deregister_denom_response_samples() {
        let ok: AcknowledgementMsg<DeregisterDenomAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<DeregisterDenomAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::add_liquidity_response_samples() {
        let ok: AcknowledgementMsg<AddLiquidityAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<AddLiquidityAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::concentrated_add_liquidity_response_samples() {
        let ok: AcknowledgementMsg<AddConcentratedLiquidityAckMsg> =
            AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding(
            "AcknowledgementMsg<AddConcentratedLiquidityAckMsg>::Ok",
            &ok,
        );
    }

    for sample in common::ack_samples::remove_liquidity_response_samples() {
        let ok: AcknowledgementMsg<RemoveLiquidityAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<RemoveLiquidityAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::concentrated_remove_liquidity_response_samples() {
        let ok: AcknowledgementMsg<RemoveConcentratedLiquidityAckMsg> =
            AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding(
            "AcknowledgementMsg<RemoveConcentratedLiquidityAckMsg>::Ok",
            &ok,
        );
    }

    for sample in common::ack_samples::concentrated_collect_fees_response_samples() {
        let ok: AcknowledgementMsg<CollectConcentratedFeesAckMsg> =
            AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<CollectConcentratedFeesAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::concentrated_collect_protocol_fees_response_samples() {
        let ok: AcknowledgementMsg<CollectConcentratedProtocolFeesAckMsg> =
            AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding(
            "AcknowledgementMsg<CollectConcentratedProtocolFeesAckMsg>::Ok",
            &ok,
        );
    }

    for sample in common::ack_samples::swap_response_samples() {
        let ok: AcknowledgementMsg<SwapAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<SwapAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::transfer_voucher_response_samples() {
        let ok: AcknowledgementMsg<TransferVoucherAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<TransferVoucherAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::deposit_token_response_samples() {
        let ok: AcknowledgementMsg<DepositTokenAckMsg> = AcknowledgementMsg::Ok(sample.into());
        assert_cross_encoding("AcknowledgementMsg<DepositTokenAckMsg>::Ok", &ok);
    }

    for sample in common::ack_samples::sentinel_samples() {
        let ok: AcknowledgementMsg<Vec<u8>> = AcknowledgementMsg::Ok(sample);
        assert_cross_encoding("AcknowledgementMsg<Vec<u8>>::sentinel", &ok);
    }
}
