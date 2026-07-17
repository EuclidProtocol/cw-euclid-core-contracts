//! §9.3: every `(uint8 tag, bytes)` enum envelope in the crate must reject an
//! out-of-range tag with `EncodingError::UnknownDiscriminant { type_name,
//! discriminant }`, matched exactly (both fields).
//!
//! Two mechanisms, because the wire layer splits enum handling in two:
//! - The message-family envelopes (`RouterReceiveMsg`, `FactoryReceiveMsg`,
//!   `AcknowledgementMsg<S>`) implement `AbiDecode`, so a hand-encoded raw
//!   `(uint8, bytes)` ABI tuple is fed through `from_abi_bytes`. The envelope
//!   shape (`tagged()`/`TaggedSol`) is crate-private, so the tuple is
//!   hand-encoded locally with `alloy_sol_types`.
//! - The shared tagged types (`TokenType`, `Limit`, `PoolType`, `PoolConfig`,
//!   `RegisterFactoryChainType`) are now free-function codecs in
//!   `wire::types::*`, not `AbiMap` impls, so the `*_from_sol` function is
//!   called directly with a synthetic `(tag, empty-bytes)` `TaggedSol` value.
//!   Their `type_name` strings are unchanged from the old `AbiMap` impls.

use alloy_sol_types::private::Bytes;
use alloy_sol_types::private::Bytes as SolBytesValue;
use alloy_sol_types::sol_data::{Bytes as SolBytes, Uint as SolUint};
use alloy_sol_types::SolType;
use std::fmt::Debug;

use euclid_encoding::{AbiDecode, EncodingError};
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::envelope::router::RouterReceiveMsg;
use euclid_ibc::wire::envelope::AcknowledgementMsg;
use euclid_ibc::wire::msgs::RegisterFactoryAckMsg;
use euclid_ibc::wire::types::limit::limit_from_sol;
use euclid_ibc::wire::types::pool_config::pool_config_from_sol;
use euclid_ibc::wire::types::pool_type::pool_type_from_sol;
use euclid_ibc::wire::types::register_factory_chain::register_factory_chain_type_from_sol;
use euclid_ibc::wire::types::token_type::token_type_from_sol;

/// Raw `(uint8, bytes)` tuple, hand-encoded independently of the crate's
/// internal `TaggedSol`/`tagged()`.
type RawTagged = (SolUint<8>, SolBytes);

fn raw_tagged(tag: u8) -> Vec<u8> {
    <RawTagged>::abi_encode_params(&(tag, SolBytesValue::new()))
}

fn assert_envelope_unknown_discriminant<T: AbiDecode + Debug>(type_name: &'static str, tag: u8) {
    let bytes = raw_tagged(tag);
    let err = T::from_abi_bytes(&bytes)
        .expect_err(&format!("tag {tag} must be rejected for {type_name}"));
    assert_eq!(
        err,
        EncodingError::UnknownDiscriminant {
            type_name,
            discriminant: tag,
        }
    );
}

/// The shared tagged types decode through their `*_from_sol` free function,
/// fed a synthetic `(tag, empty-bytes)` `TaggedSol` value directly.
fn assert_from_sol_unknown_discriminant<T: Debug>(
    type_name: &'static str,
    tag: u8,
    result: Result<T, EncodingError>,
) {
    let err = result.expect_err(&format!("tag {tag} must be rejected for {type_name}"));
    assert_eq!(
        err,
        EncodingError::UnknownDiscriminant {
            type_name,
            discriminant: tag,
        }
    );
}

#[test]
fn router_rejects_variant_count_and_255() {
    // 14 variants (tags 0..=13); one past the last valid tag is 14.
    assert_envelope_unknown_discriminant::<RouterReceiveMsg>("RouterReceiveMsg", 14);
    assert_envelope_unknown_discriminant::<RouterReceiveMsg>("RouterReceiveMsg", 255);
}

#[test]
fn factory_rejects_variant_count_and_255() {
    // 2 variants (tags 0..=1).
    assert_envelope_unknown_discriminant::<FactoryReceiveMsg>("FactoryReceiveMsg", 2);
    assert_envelope_unknown_discriminant::<FactoryReceiveMsg>("FactoryReceiveMsg", 255);
}

#[test]
fn ack_envelope_rejects_variant_count_and_255() {
    // 2 arms (Ok = 0, Error = 1); `S` is arbitrary here, any concrete wire
    // mirror satisfying the generic `AbiEncode + AbiDecode` bound works.
    assert_envelope_unknown_discriminant::<AcknowledgementMsg<RegisterFactoryAckMsg>>(
        "AcknowledgementMsg",
        2,
    );
    assert_envelope_unknown_discriminant::<AcknowledgementMsg<RegisterFactoryAckMsg>>(
        "AcknowledgementMsg",
        255,
    );
}

#[test]
fn token_type_rejects_variant_count_and_255() {
    // 3 variants: Native, Smart, Voucher.
    assert_from_sol_unknown_discriminant("TokenType", 3, token_type_from_sol((3, Bytes::new())));
    assert_from_sol_unknown_discriminant(
        "TokenType",
        255,
        token_type_from_sol((255, Bytes::new())),
    );
}

#[test]
fn limit_rejects_variant_count_and_255() {
    // 4 variants: LessThanOrEqual, Equal, GreaterThanOrEqual, Dynamic.
    assert_from_sol_unknown_discriminant("Limit", 4, limit_from_sol((4, Bytes::new())));
    assert_from_sol_unknown_discriminant("Limit", 255, limit_from_sol((255, Bytes::new())));
}

#[test]
fn pool_type_rejects_variant_count_and_255() {
    // 3 variants: ConstantProduct, Stable, Concentrated.
    assert_from_sol_unknown_discriminant("PoolType", 3, pool_type_from_sol((3, Bytes::new())));
    assert_from_sol_unknown_discriminant("PoolType", 255, pool_type_from_sol((255, Bytes::new())));
}

#[test]
fn pool_config_rejects_variant_count_and_255() {
    // 3 variants: Stable, ConstantProduct, Concentrated.
    assert_from_sol_unknown_discriminant("PoolConfig", 3, pool_config_from_sol((3, Bytes::new())));
    assert_from_sol_unknown_discriminant(
        "PoolConfig",
        255,
        pool_config_from_sol((255, Bytes::new())),
    );
}

#[test]
fn register_factory_chain_type_rejects_variant_count_and_255() {
    // 4 variants: Native, Cosmos, Evm, Tvm.
    assert_from_sol_unknown_discriminant(
        "RegisterFactoryChainType",
        4,
        register_factory_chain_type_from_sol((4, Bytes::new())),
    );
    assert_from_sol_unknown_discriminant(
        "RegisterFactoryChainType",
        255,
        register_factory_chain_type_from_sol((255, Bytes::new())),
    );
}
