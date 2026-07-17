//! JSON byte parity between the domain serde and the wire serde for every ack
//! sample. The wire ack mirrors exist so the same bytes cross the wire whether
//! a handler serializes the domain response or its wire mirror; this suite
//! pins `to_json_vec(domain) == to_json_vec(wire)` for all thirteen ack
//! mirrors, each compared both bare and wrapped in `AcknowledgementMsg::Ok`.
//! (Pool creation has no dedicated wire ack mirror: creation chains into the
//! initial liquidity add, so its acks travel as the add liquidity mirrors.)
//! (The send envelopes need no parity suite since the deletion of the legacy
//! domain enums: the wire enums are the only serialized form, pinned by the
//! JSON snapshot tests in `euclid_ibc::wire::envelope`.)

mod common;

use cosmwasm_std::to_json_vec;
use euclid_ibc::wire::envelope::AcknowledgementMsg;
use euclid_ibc::wire::msgs::{
    AddConcentratedLiquidityAckMsg, AddLiquidityAckMsg, CollectConcentratedFeesAckMsg,
    CollectConcentratedProtocolFeesAckMsg, DepositTokenAckMsg, DeregisterDenomAckMsg,
    RegisterDenomAckMsg, RegisterFactoryAckMsg, ReleaseEscrowAckMsg,
    RemoveConcentratedLiquidityAckMsg, RemoveLiquidityAckMsg, SwapAckMsg, TransferVoucherAckMsg,
};
use serde::Serialize;

/// The bare mirror and, wrapped in `AcknowledgementMsg::Ok`, the envelope must
/// both serialize byte-identically to their domain counterparts.
fn assert_ack_json_parity<D, W>(samples: Vec<D>)
where
    D: Serialize + Clone,
    W: Serialize + From<D>,
{
    assert!(!samples.is_empty(), "sample list must not be empty");
    for domain in samples {
        let wire: W = domain.clone().into();
        assert_eq!(
            to_json_vec(&domain).unwrap(),
            to_json_vec(&wire).unwrap(),
            "bare mirror json bytes differ"
        );

        let domain_ok = AcknowledgementMsg::Ok(domain);
        let wire_ok = AcknowledgementMsg::Ok(wire);
        assert_eq!(
            to_json_vec(&domain_ok).unwrap(),
            to_json_vec(&wire_ok).unwrap(),
            "Ok-wrapped envelope json bytes differ"
        );
    }
}

#[test]
fn ack_mirrors_json_are_byte_identical() {
    assert_ack_json_parity::<_, RegisterFactoryAckMsg>(
        common::ack_samples::register_factory_response_samples(),
    );
    assert_ack_json_parity::<_, ReleaseEscrowAckMsg>(
        common::ack_samples::release_escrow_response_samples(),
    );
    assert_ack_json_parity::<_, RegisterDenomAckMsg>(
        common::ack_samples::register_denom_response_samples(),
    );
    assert_ack_json_parity::<_, DeregisterDenomAckMsg>(
        common::ack_samples::deregister_denom_response_samples(),
    );
    // Pool creation has no dedicated wire ack mirror: creation chains into the
    // initial liquidity add, so the wire carries the add liquidity acks (tags
    // 4/5), covered by the AddLiquidity / AddConcentratedLiquidity cases below.
    assert_ack_json_parity::<_, AddLiquidityAckMsg>(
        common::ack_samples::add_liquidity_response_samples(),
    );
    assert_ack_json_parity::<_, AddConcentratedLiquidityAckMsg>(
        common::ack_samples::concentrated_add_liquidity_response_samples(),
    );
    assert_ack_json_parity::<_, RemoveLiquidityAckMsg>(
        common::ack_samples::remove_liquidity_response_samples(),
    );
    assert_ack_json_parity::<_, RemoveConcentratedLiquidityAckMsg>(
        common::ack_samples::concentrated_remove_liquidity_response_samples(),
    );
    assert_ack_json_parity::<_, CollectConcentratedFeesAckMsg>(
        common::ack_samples::concentrated_collect_fees_response_samples(),
    );
    assert_ack_json_parity::<_, CollectConcentratedProtocolFeesAckMsg>(
        common::ack_samples::concentrated_collect_protocol_fees_response_samples(),
    );
    assert_ack_json_parity::<_, SwapAckMsg>(common::ack_samples::swap_response_samples());
    assert_ack_json_parity::<_, TransferVoucherAckMsg>(
        common::ack_samples::transfer_voucher_response_samples(),
    );
    assert_ack_json_parity::<_, DepositTokenAckMsg>(
        common::ack_samples::deposit_token_response_samples(),
    );
}
