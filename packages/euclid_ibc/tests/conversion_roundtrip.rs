//! Domain -> wire -> domain identity for every ack mirror family. The wire
//! layer's contract is that converting a domain response to its wire ack
//! mirror and back is lossless; this suite pins that for all thirteen ack
//! mirrors. (Pool creation has no dedicated wire ack mirror: creation chains
//! into the initial liquidity add, so its acks travel as the add liquidity
//! mirrors.) Driven by the shared `common` sample builders. (The send side has
//! no domain counterpart since the deletion of the legacy domain enums; the
//! wire send enums are covered by the encoding roundtrip suites.)

mod common;

use std::fmt::Debug;

use euclid_ibc::wire::msgs::{
    AddConcentratedLiquidityAckMsg, AddLiquidityAckMsg, CollectConcentratedFeesAckMsg,
    CollectConcentratedProtocolFeesAckMsg, DepositTokenAckMsg, DeregisterDenomAckMsg,
    RegisterDenomAckMsg, RegisterFactoryAckMsg, ReleaseEscrowAckMsg,
    RemoveConcentratedLiquidityAckMsg, RemoveLiquidityAckMsg, SwapAckMsg, TransferVoucherAckMsg,
};

/// `domain -> W -> domain` must be the identity, for a mirror `W` with `From`
/// impls in both directions.
fn assert_mirror_roundtrip<D, W>(samples: Vec<D>)
where
    D: Clone + PartialEq + Debug + From<W>,
    W: From<D>,
{
    assert!(!samples.is_empty(), "sample list must not be empty");
    for domain in samples {
        let wire: W = domain.clone().into();
        let back: D = wire.into();
        assert_eq!(domain, back, "domain -> wire -> domain mismatch");
    }
}

#[test]
fn ack_mirrors_roundtrip_domain_wire_domain() {
    assert_mirror_roundtrip::<_, RegisterFactoryAckMsg>(
        common::ack_samples::register_factory_response_samples(),
    );
    assert_mirror_roundtrip::<_, ReleaseEscrowAckMsg>(
        common::ack_samples::release_escrow_response_samples(),
    );
    assert_mirror_roundtrip::<_, RegisterDenomAckMsg>(
        common::ack_samples::register_denom_response_samples(),
    );
    assert_mirror_roundtrip::<_, DeregisterDenomAckMsg>(
        common::ack_samples::deregister_denom_response_samples(),
    );
    // Pool creation has no dedicated wire ack mirror: creation chains into the
    // initial liquidity add, so the wire carries the add liquidity acks (tags
    // 4/5), covered by the AddLiquidity / AddConcentratedLiquidity cases below.
    assert_mirror_roundtrip::<_, AddLiquidityAckMsg>(
        common::ack_samples::add_liquidity_response_samples(),
    );
    assert_mirror_roundtrip::<_, AddConcentratedLiquidityAckMsg>(
        common::ack_samples::concentrated_add_liquidity_response_samples(),
    );
    assert_mirror_roundtrip::<_, RemoveLiquidityAckMsg>(
        common::ack_samples::remove_liquidity_response_samples(),
    );
    assert_mirror_roundtrip::<_, RemoveConcentratedLiquidityAckMsg>(
        common::ack_samples::concentrated_remove_liquidity_response_samples(),
    );
    assert_mirror_roundtrip::<_, CollectConcentratedFeesAckMsg>(
        common::ack_samples::concentrated_collect_fees_response_samples(),
    );
    assert_mirror_roundtrip::<_, CollectConcentratedProtocolFeesAckMsg>(
        common::ack_samples::concentrated_collect_protocol_fees_response_samples(),
    );
    assert_mirror_roundtrip::<_, SwapAckMsg>(common::ack_samples::swap_response_samples());
    assert_mirror_roundtrip::<_, TransferVoucherAckMsg>(
        common::ack_samples::transfer_voucher_response_samples(),
    );
    assert_mirror_roundtrip::<_, DepositTokenAckMsg>(
        common::ack_samples::deposit_token_response_samples(),
    );
}
