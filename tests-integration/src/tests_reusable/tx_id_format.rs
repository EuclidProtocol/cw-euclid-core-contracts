#![cfg(not(target_arch = "wasm32"))]

//! Layer 1 end-to-end coverage for the tx_id reorg-safety fix
//! (SC-8 CosmWasm slice). Exercises the new format and per-sender
//! `TX_NONCES` map through real cross-chain flows driven by
//! `cw-orch-interchain`. The harness cannot reorg, so these tests
//! cover what unit tests can't: that the new code wires through
//! correctly under a full Factory → Router → ack round-trip.

#[cfg(test)]
mod tests {
    use cosmwasm_std::from_json;
    use cw_orch::mock::MockBase;
    use cw_orch::prelude::{ContractInstance, Environment};
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::msg::ExecuteMsgFns as FactoryExecuteMsgFns;
    use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::token::{Token, TokenType, TokenWithDenom};
    use euclid::utils::tx::TX_NONCES;
    use euclid_ibc::router_ibc::RouterCrossChainExecuteMsg;
    use factory::FactoryContract;

    use crate::helpers::chains::{setup_factory, setup_interchain, setup_router};
    use crate::helpers::relayer::{
        extract_send_packet_events, relay_factory_ack_packet, relay_factory_send_packet,
    };
    use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID};

    /// Top-level segments of the new format: sender expands to
    /// `{chain_uid}:{address}`, then `:chain_id:nonce` → 4 total.
    const NEW_FORMAT_COLON_SEGMENT_COUNT: usize = 4;

    fn make_token(seed: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(seed.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: seed.to_string(),
                decimals: Some(18),
            },
        }
    }

    /// Returns `(tx_id_from_outbound_packet, raw_send_events)` from a
    /// `register_denom` factory call. The events vec is what the ack-relay
    /// helper expects.
    fn register_and_capture_tx_id(
        factory: &FactoryContract<MockBase>,
        seed: &str,
    ) -> (String, Vec<cosmwasm_std::Event>) {
        let token = make_token(seed);
        let tx = factory
            .register_denom(CrossChainConfig::default(), token)
            .unwrap();
        let packet = extract_send_packet_events(&tx.events)
            .into_iter()
            .next()
            .expect("expected one send_packet event");
        let msg: RouterCrossChainExecuteMsg = from_json(&packet.msg).unwrap();
        (msg.get_tx_id(), tx.events)
    }

    /// The new format must have exactly 4 colon-separated segments
    /// (`{sender_chain_uid}:{sender_address}:{chain_id}:{nonce}`),
    /// confirming `block.height` and `transaction.index` are absent.
    #[test]
    fn tx_id_emitted_in_packet_has_new_four_segment_shape() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();

        let (tx_id, _) = register_and_capture_tx_id(&factory, "shapecheck");

        let segments: Vec<&str> = tx_id.split(':').collect();
        assert_eq!(
            segments.len(),
            NEW_FORMAT_COLON_SEGMENT_COUNT,
            "tx_id should have {NEW_FORMAT_COLON_SEGMENT_COUNT} segments: tx_id={tx_id}"
        );
        // The last segment is the nonce — must parse as u128.
        let nonce: u128 = segments
            .last()
            .unwrap()
            .parse()
            .unwrap_or_else(|_| panic!("expected nonce in tx_id, got: {tx_id}"));
        assert_eq!(nonce, 1, "factory's first call should produce nonce=1");
    }

    /// After a Factory call, `TX_NONCES` (keyed by `{chain_uid}:{address}`)
    /// must contain the expected per-sender counter.
    #[test]
    fn tx_nonces_storage_is_populated_per_sender_after_cross_chain_call() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();

        let (tx_id_1, _) = register_and_capture_tx_id(&factory, "nonceone");

        // tx_id format: "{chain_uid}:{address}:{chain_id}:{nonce}".
        // TX_NONCES key is the first two segments joined by ':'.
        let mut segments = tx_id_1.split(':');
        let sender_key = format!("{}:{}", segments.next().unwrap(), segments.next().unwrap(),);

        let stored = TX_NONCES
            .query(
                &factory.environment().app.borrow().wrap(),
                factory.address().unwrap(),
                sender_key.clone(),
            )
            .unwrap();
        assert_eq!(stored, Some(1u128), "expected TX_NONCES[{sender_key}] == 1");
    }

    /// Two sequential calls from the same sender should produce nonces 1 then 2
    /// — the per-sender counter increments monotonically and is visible end-to-end
    /// through the full Factory → Router → ack round-trip with storage committed.
    #[test]
    fn tx_nonces_increments_across_sequential_calls() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();
        let factory_chain_uid = factory.get_state().unwrap().chain_uid;

        // First call (and full round-trip so storage is committed).
        let (tx_id_1, events_1) = register_and_capture_tx_id(&factory, "incrone");
        let ack_events_1 = relay_factory_send_packet(events_1, &router).unwrap();
        relay_factory_ack_packet(&factory, ack_events_1, &factory_chain_uid).unwrap();

        // Second call.
        let (tx_id_2, events_2) = register_and_capture_tx_id(&factory, "incrtwo");
        let ack_events_2 = relay_factory_send_packet(events_2, &router).unwrap();
        relay_factory_ack_packet(&factory, ack_events_2, &factory_chain_uid).unwrap();

        assert!(
            tx_id_1.ends_with(":1"),
            "first tx_id should end with nonce 1: {tx_id_1}"
        );
        assert!(
            tx_id_2.ends_with(":2"),
            "second tx_id should end with nonce 2: {tx_id_2}"
        );

        // Cross-check via storage.
        let mut segments = tx_id_1.split(':');
        let sender_key = format!("{}:{}", segments.next().unwrap(), segments.next().unwrap(),);
        let stored = TX_NONCES
            .query(
                &factory.environment().app.borrow().wrap(),
                factory.address().unwrap(),
                sender_key,
            )
            .unwrap();
        assert_eq!(stored, Some(2u128));
    }

    /// Reorg-safety regression: the third segment must be the chain_id, not a
    /// numeric block height. Locks down field ordering against an accidental
    /// regression that would put `block.height` back in.
    #[test]
    fn tx_id_third_segment_is_chain_id_not_block_height() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();

        let (tx_id, _) = register_and_capture_tx_id(&factory, "thirdseg");

        let segments: Vec<&str> = tx_id.split(':').collect();
        assert_eq!(segments.len(), NEW_FORMAT_COLON_SEGMENT_COUNT);
        assert_eq!(
            segments[2], FACTORY_CHAIN_ID_IBC,
            "third segment should be chain_id, got: tx_id={tx_id}"
        );
    }

    /// Migration safety: the old `Item<u128>` at raw storage key `"tx_nonce"`
    /// must not be written by the new code. In production, an upgraded contract
    /// instance may still carry a residual value at this key from a previous
    /// deployment (orphan storage); this test pins the property that the *new*
    /// code never writes there, which is what makes the orphan harmless.
    #[test]
    fn old_global_tx_nonce_key_is_never_written_by_new_code() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();
        let factory_chain_uid = factory.get_state().unwrap().chain_uid;

        // Two full round-trips through the path that previously wrote to the
        // global TX_NONCE Item.
        let (_, events_1) = register_and_capture_tx_id(&factory, "orphanone");
        let ack_1 = relay_factory_send_packet(events_1, &router).unwrap();
        relay_factory_ack_packet(&factory, ack_1, &factory_chain_uid).unwrap();
        let (_, events_2) = register_and_capture_tx_id(&factory, "orphantwo");
        let ack_2 = relay_factory_send_packet(events_2, &router).unwrap();
        relay_factory_ack_packet(&factory, ack_2, &factory_chain_uid).unwrap();

        // Raw-storage read on the old Item key: cw-storage-plus stores
        // `Item::new("tx_nonce")` at exactly that byte sequence.
        let raw = factory
            .environment()
            .app
            .borrow()
            .wrap()
            .query_wasm_raw(factory.address().unwrap(), b"tx_nonce")
            .unwrap();
        assert!(
            raw.is_none(),
            "old global TX_NONCE key must not be written by new code"
        );
    }
}
