//! §9.3: for every message family (`RouterReceiveMsg`, `FactoryReceiveMsg`,
//! `AcknowledgementMsg<S>` for every wire ack mirror), ABI-encode a sample and
//! attempt to decode every strict prefix at word boundaries (0, 32, 64, half
//! the buffer) and the buffer minus one byte. Every case must return
//! `EncodingError::AbiDecode`, never panic, and never `Ok`.
//!
//! ## The alloy tolerance quirk, why the envelope absorbs it
//!
//! `alloy-sol-types` does not require a dynamic field's ABI-mandated
//! zero-padding tail to be physically present in the buffer: if the last
//! dynamic field's actual (unpadded) data is fully present, decode succeeds
//! even if the trailing padding bytes that word-align it are missing. That
//! quirk cannot manifest for any of the three message families here: every
//! family's top-level ABI shape is the `(uint8 tag, bytes payload)` envelope,
//! and the `payload` field's actual content is itself the output of
//! `abi_encode_params` on the variant's inner tuple, which is always a
//! multiple of 32 bytes. So the outer `bytes` field needs zero padding, there
//! is no padding window to truncate into, and any missing byte removes real,
//! load-bearing content. Truncation always fails on these enveloped messages.

// `common` is declared once, in `main.rs` (the crate root for this
// integration-test binary); reachable here as `crate::common`.
use crate::common;

use std::fmt::Debug;

use euclid_encoding::{AbiDecode, AbiEncode, EncodingError};
use euclid_ibc::wire::envelope::factory::FactoryReceiveMsg;
use euclid_ibc::wire::envelope::router::RouterReceiveMsg;
use euclid_ibc::wire::envelope::AcknowledgementMsg;

/// The plan's exact checkpoint set: 0, 32, 64, half the buffer, and the
/// buffer minus one byte. Clipped and deduplicated for buffers shorter than
/// 64 bytes.
fn checkpoints(len: usize) -> Vec<usize> {
    let mut points = vec![0, 32.min(len), 64.min(len), len / 2, len.saturating_sub(1)];
    points.sort_unstable();
    points.dedup();
    points
}

fn assert_all_prefixes_reject<T>(label: &str, value: &T)
where
    T: AbiEncode + AbiDecode + Debug,
{
    let full = value.to_abi_bytes().unwrap_or_else(|e| {
        panic!("abi encode failed for {label}: {e}");
    });
    for prefix_len in checkpoints(full.len()) {
        let prefix = &full[..prefix_len];
        match T::from_abi_bytes(prefix) {
            Err(EncodingError::AbiDecode { .. }) => {}
            Err(other) => panic!(
                "{label}: prefix len {prefix_len}/{} returned the wrong error variant: {other:?}",
                full.len()
            ),
            Ok(decoded) => panic!(
                "{label}: prefix len {prefix_len}/{} unexpectedly decoded Ok: {decoded:?}",
                full.len()
            ),
        }
    }
}

#[test]
fn router_family_rejects_all_truncated_prefixes() {
    for (label, msg) in common::router_samples::all_samples() {
        assert_all_prefixes_reject::<RouterReceiveMsg>(label, &msg);
    }
}

#[test]
fn factory_family_rejects_all_truncated_prefixes() {
    for msg in common::factory_samples::all_samples() {
        assert_all_prefixes_reject::<FactoryReceiveMsg>("factory-sample", &msg);
    }
}

/// Every one of the 13 typed wire ack mirrors (plan §2.3), each wrapped in
/// `Ok`, plus one `Error` arm and the sentinel, all through the generic
/// `AcknowledgementMsg<S>` envelope. (Pool creation has no dedicated wire ack
/// mirror: creation chains into the initial liquidity add, so its acks travel
/// as the add liquidity mirrors, covered by those cases.)
#[test]
fn ack_family_rejects_all_truncated_prefixes() {
    fn check_ok_from<D, W>(label: &str, domain_samples: Vec<D>)
    where
        W: From<D> + AbiEncode + AbiDecode + Debug,
    {
        for sample in domain_samples {
            let ok: AcknowledgementMsg<W> = AcknowledgementMsg::Ok(sample.into());
            assert_all_prefixes_reject(label, &ok);
        }
    }

    check_ok_from::<_, euclid_ibc::wire::msgs::RegisterFactoryAckMsg>(
        "RegisterFactoryAckMsg",
        common::ack_samples::register_factory_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::ReleaseEscrowAckMsg>(
        "ReleaseEscrowAckMsg",
        common::ack_samples::release_escrow_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::RegisterDenomAckMsg>(
        "RegisterDenomAckMsg",
        common::ack_samples::register_denom_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::DeregisterDenomAckMsg>(
        "DeregisterDenomAckMsg",
        common::ack_samples::deregister_denom_response_samples(),
    );
    // Pool creation has no dedicated wire ack mirror: creation chains into the
    // initial liquidity add, so the wire carries the add liquidity acks (tags
    // 4/5), covered by the AddLiquidity / AddConcentratedLiquidity cases below.
    check_ok_from::<_, euclid_ibc::wire::msgs::AddLiquidityAckMsg>(
        "AddLiquidityAckMsg",
        common::ack_samples::add_liquidity_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::AddConcentratedLiquidityAckMsg>(
        "AddConcentratedLiquidityAckMsg",
        common::ack_samples::concentrated_add_liquidity_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::RemoveLiquidityAckMsg>(
        "RemoveLiquidityAckMsg",
        common::ack_samples::remove_liquidity_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::RemoveConcentratedLiquidityAckMsg>(
        "RemoveConcentratedLiquidityAckMsg",
        common::ack_samples::concentrated_remove_liquidity_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::CollectConcentratedFeesAckMsg>(
        "CollectConcentratedFeesAckMsg",
        common::ack_samples::concentrated_collect_fees_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::CollectConcentratedProtocolFeesAckMsg>(
        "CollectConcentratedProtocolFeesAckMsg",
        common::ack_samples::concentrated_collect_protocol_fees_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::SwapAckMsg>(
        "SwapAckMsg",
        common::ack_samples::swap_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::TransferVoucherAckMsg>(
        "TransferVoucherAckMsg",
        common::ack_samples::transfer_voucher_response_samples(),
    );
    check_ok_from::<_, euclid_ibc::wire::msgs::DepositTokenAckMsg>(
        "DepositTokenAckMsg",
        common::ack_samples::deposit_token_response_samples(),
    );
    // The sentinel has no wire mirror; `From<Vec<u8>>` resolves to identity.
    check_ok_from::<_, Vec<u8>>("sentinel", common::ack_samples::sentinel_samples());

    // One `Error` arm, to prove the envelope's other tag is covered too.
    let error: AcknowledgementMsg<euclid_ibc::wire::msgs::RegisterFactoryAckMsg> =
        AcknowledgementMsg::Error("boom".to_string());
    assert_all_prefixes_reject("AcknowledgementMsg::Error", &error);
}
