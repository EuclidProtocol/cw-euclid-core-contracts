//! §9.3: unit enum variants and empty-struct ack responses carry an empty ABI
//! payload by definition (§6/§7.6). A *valid* tag paired with a *non-empty*
//! payload must be rejected with `NonEmptyPayload`, never silently accepted
//! (`ensure_empty`/the hand-rolled empty-struct impls are the crate's only
//! guard against garbage smuggled into an otherwise-ignored slot).
//!
//! `PoolType`, `PoolConfig`, and `TokenType` are now free-function codecs in
//! `wire::types`, so their unit arms are exercised by calling `*_from_sol`
//! with a synthetic `(valid-tag, non-empty-bytes)` `TaggedSol` value. The
//! empty ack mirrors (`RegisterDenomAckMsg`, `DeregisterDenomAckMsg`) are each
//! reached through the *generic* `AcknowledgementMsg<S>` envelope's ABI
//! decode, proving the envelope's `Ok` arm propagates the inner decode error
//! instead of swallowing it.

use alloy_sol_types::private::Bytes;
use alloy_sol_types::private::Bytes as SolBytesValue;
use alloy_sol_types::sol_data::{Bytes as SolBytes, Uint as SolUint};
use alloy_sol_types::SolType;

use euclid_encoding::{AbiDecode, EncodingError};
use euclid_ibc::wire::envelope::AcknowledgementMsg;
use euclid_ibc::wire::msgs::{DeregisterDenomAckMsg, RegisterDenomAckMsg};
use euclid_ibc::wire::types::pool_config::pool_config_from_sol;
use euclid_ibc::wire::types::pool_type::pool_type_from_sol;
use euclid_ibc::wire::types::token_type::token_type_from_sol;

type RawTagged = (SolUint<8>, SolBytes);

fn raw_tagged(tag: u8, payload: Vec<u8>) -> Vec<u8> {
    <RawTagged>::abi_encode_params(&(tag, SolBytesValue::from(payload)))
}

#[test]
fn pool_type_constant_product_rejects_nonempty_payload() {
    let err = pool_type_from_sol((0, Bytes::from(vec![1, 2, 3]))).unwrap_err();
    assert_eq!(
        err,
        EncodingError::NonEmptyPayload {
            type_name: "PoolType",
            len: 3
        }
    );
}

#[test]
fn pool_type_stable_rejects_nonempty_payload() {
    let err = pool_type_from_sol((1, Bytes::from(vec![9]))).unwrap_err();
    assert_eq!(
        err,
        EncodingError::NonEmptyPayload {
            type_name: "PoolType",
            len: 1
        }
    );
}

#[test]
fn pool_config_constant_product_rejects_nonempty_payload() {
    // ConstantProduct is tag 1 in PoolConfig's declaration order (Stable = 0,
    // ConstantProduct = 1, Concentrated = 2; see plan §6.1).
    let err = pool_config_from_sol((1, Bytes::from(vec![1, 2]))).unwrap_err();
    assert_eq!(
        err,
        EncodingError::NonEmptyPayload {
            type_name: "PoolConfig",
            len: 2
        }
    );
}

#[test]
fn token_type_voucher_rejects_nonempty_payload() {
    // Voucher is tag 2 in TokenType's declaration order (Native = 0,
    // Smart = 1, Voucher = 2; see src/wire/types/token_type.rs). It is the
    // only unit variant of a non-unit-only enum, and the `ensure_empty` guard
    // it goes through had no negative-path test before this one.
    let err = token_type_from_sol((2, Bytes::from(vec![4, 5, 6]))).unwrap_err();
    assert_eq!(
        err,
        EncodingError::NonEmptyPayload {
            type_name: "TokenType",
            len: 3
        }
    );
}

/// The empty-ack error must propagate through the generic ack envelope
/// (`AcknowledgementMsg<S>::from_abi_bytes`'s `Ok` arm calls
/// `S::from_abi_bytes` on the inner bytes), not just when `RegisterDenomAckMsg`
/// is decoded directly.
#[test]
fn register_denom_ack_nonempty_payload_rejected_through_ack_envelope() {
    let inner = vec![7, 7, 7];
    let bytes = raw_tagged(0, inner.clone());
    let err = AcknowledgementMsg::<RegisterDenomAckMsg>::from_abi_bytes(&bytes).unwrap_err();
    assert_eq!(
        err,
        EncodingError::NonEmptyPayload {
            type_name: "RegisterDenomAckMsg",
            len: inner.len(),
        }
    );
}

/// Mirrors `register_denom_ack_nonempty_payload_rejected_through_ack_envelope`
/// for the other hand-rolled empty ack (`DeregisterDenomAckMsg`), so both
/// empty-struct `AbiDecode` impls are proven to propagate through the generic
/// envelope, not just one of them.
#[test]
fn deregister_denom_ack_nonempty_payload_rejected_through_ack_envelope() {
    let inner = vec![8, 8, 8];
    let bytes = raw_tagged(0, inner.clone());
    let err = AcknowledgementMsg::<DeregisterDenomAckMsg>::from_abi_bytes(&bytes).unwrap_err();
    assert_eq!(
        err,
        EncodingError::NonEmptyPayload {
            type_name: "DeregisterDenomAckMsg",
            len: inner.len(),
        }
    );
}
