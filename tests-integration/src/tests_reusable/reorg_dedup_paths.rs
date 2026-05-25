#![cfg(not(target_arch = "wasm32"))]

//! Integration coverage for the reorg dedup paths that this fix relies on
//! (SC-8 CosmWasm slice). These tests exercise the contract-level dedup
//! guards through real Factory → Router → ack round-trips driven by
//! `cw-orch-interchain`, complementing the unit-level coverage of the
//! same paths in `router/src/execute/relay.rs` and the per-operation
//! `TxAlreadyExist` checks in the factory.
//!
//! The cw-orch mock cannot actually reorg; the redelivery helpers in
//! `helpers::relayer` fabricate the duplicate that a reorgged source would
//! emit by re-signing the same packet with a fresh relayer-meta-tx nonce
//! — that bypasses the relayer-level nonce dedup so the *contract-level*
//! rejection path is what the assertion observes.
//!
//! Not covered here (unit-only): the Router-side `TxAlreadyExist` guard at
//! `router/ibc/receive/swap.rs:48-51`. Exercising it end-to-end would
//! require either a fully-funded VLP pool (so the inner `ibc_execute_swap`
//! actually persists its `PENDING_SWAPS` row before the dedup fires on the
//! replay) or a hand-fabricated swap packet plus pre-seeded router state.
//! The unit test `test_ibc_swap_same_tx_id_rejected_with_tx_already_exist`
//! exercises the guard directly with `MockDeps`, which the reviewer noted
//! is sufficient for cases like this.

#[cfg(test)]
mod tests {
    use cw_orch_interchain::prelude::InterchainEnv;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::msg::ExecuteMsgFns as FactoryExecuteMsgFns;
    use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
    use euclid::token::{Token, TokenType, TokenWithDenom};

    use cosmwasm_std::Addr;
    use cw_orch::prelude::{ContractInstance, Environment};

    use crate::helpers::chains::{setup_factory, setup_interchain, setup_router};
    use crate::helpers::relayer::{
        redeliver_factory_ack_packet, redeliver_factory_send_packet, relay_factory_ack_packet,
        relay_factory_send_packet,
    };
    use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID};

    fn make_token(seed: &str) -> TokenWithDenom {
        TokenWithDenom {
            token: Token::create(seed.to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: seed.to_string(),
                decimals: Some(18),
            },
        }
    }

    /// Scenario B core: relayer forwards the same Factory→Router packet twice
    /// (same `(chain_uid, sequence)`). The first delivery succeeds; the
    /// second must be rejected by Router's
    /// `CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS` guard with the contract-level
    /// "Processed sequence already exists" error.
    #[test]
    fn router_rejects_duplicate_sequence_redelivery() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();

        let tx = factory
            .register_denom(CrossChainConfig::default(), make_token("dupseq"))
            .unwrap();

        // First delivery — happy path.
        relay_factory_send_packet(tx.events.clone(), &router).unwrap();

        // Second delivery of the same (chain_uid, sequence). The relayer-level
        // nonce dedup is bypassed by the fresh suffix, so the router's
        // contract-level dedup is what surfaces.
        let err = redeliver_factory_send_packet(tx.events, &router, "reorg")
            .expect_err("router must reject duplicate sequence");
        assert!(
            format!("{err:?}").contains("Processed sequence already exists"),
            "expected router CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS rejection, got: {err}"
        );
    }

    /// Scenario B end-to-end (Factory → Router happy path, redeliver rejected,
    /// ack still works): the contract-level dedup catching a duplicate must
    /// not break the original packet's pending state, so the ack round-trip
    /// for the *first* delivery continues to succeed afterward.
    #[test]
    fn duplicate_packet_does_not_break_ack_round_trip() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();
        let factory_chain_uid = factory.get_state().unwrap().chain_uid;

        let tx = factory
            .register_denom(CrossChainConfig::default(), make_token("scenariob"))
            .unwrap();

        // First delivery emits the ack events we'll need below.
        let ack_events = relay_factory_send_packet(tx.events.clone(), &router).unwrap();

        // Duplicate delivery — must be rejected, must not perturb pending state.
        let dup = redeliver_factory_send_packet(tx.events, &router, "reorg")
            .expect_err("router must reject duplicate sequence");
        assert!(
            format!("{dup:?}").contains("Processed sequence already exists"),
            "expected router CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS rejection, got: {dup:?}"
        );

        // The original ack must still resolve cleanly on the factory side
        // (i.e. PENDING_* was preserved despite the duplicate attempt).
        relay_factory_ack_packet(&factory, ack_events, &factory_chain_uid).unwrap();
    }

    /// Scenario D-style ack re-delivery: after the factory has already
    /// processed an ack and removed its `PENDING_*` entry, a second delivery
    /// of the same ack sequence must fail rather than silently re-process.
    /// The factory's ack handler doesn't carry an explicit
    /// `CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS` check (the implicit guard is
    /// that `remove_pending_packet_and_decrement_count` errors on a missing
    /// pending packet), so the assertion here is just that the duplicate
    /// fails — no double-spend, no silent success.
    #[test]
    fn factory_rejects_duplicate_ack_after_round_trip() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();
        let factory_chain_uid = factory.get_state().unwrap().chain_uid;

        let tx = factory
            .register_denom(CrossChainConfig::default(), make_token("dupack"))
            .unwrap();
        let ack_events = relay_factory_send_packet(tx.events, &router).unwrap();

        // First ack — happy path.
        relay_factory_ack_packet(&factory, ack_events.clone(), &factory_chain_uid).unwrap();

        // Re-deliver the same ack with a fresh relayer-meta-tx nonce so the
        // factory's contract-level state is what surfaces.
        let err = redeliver_factory_ack_packet(&factory, ack_events, &factory_chain_uid, "reorg")
            .expect_err("factory must reject duplicate ack delivery");
        // Either a PENDING_* not-found, or any other clean error variant.
        // Critical property: the call did NOT silently succeed.
        let _ = err;
    }

    /// Scenario C dedup survival: after a *full* round-trip (packet + ack
    /// processed), the router still has `(chain_uid, sequence)` marked in
    /// `CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS` — so a re-delivery from a
    /// post-complete reorg is still rejected. (The released output cannot
    /// be unwound; that limitation is documented on `generate_tx`.)
    #[test]
    fn router_dedup_survives_full_round_trip() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();
        let factory_chain_uid = factory.get_state().unwrap().chain_uid;

        let tx = factory
            .register_denom(CrossChainConfig::default(), make_token("postcomp"))
            .unwrap();

        // Full round-trip first.
        let ack_events = relay_factory_send_packet(tx.events.clone(), &router).unwrap();
        relay_factory_ack_packet(&factory, ack_events, &factory_chain_uid).unwrap();

        // Now re-deliver the original send. Even though the ack has cleaned up
        // the router's `PENDING_*` entry, the processed-sequence record must
        // still cause rejection.
        let err = redeliver_factory_send_packet(tx.events, &router, "postcomplete")
            .expect_err("router must still reject after full round-trip");
        assert!(
            format!("{err:?}").contains("Processed sequence already exists"),
            "expected router CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS rejection post-ack, got: {err}"
        );
    }

    /// Scenario E: timeout race with reorg. The packet's IBC timeout fires
    /// at the router (because the source-chain wall-clock has moved past it
    /// during the reorg window), so the router's *internal* callback errors
    /// with `PacketTimedOut`. Crucially, the OUTER `execute_receive_packet`
    /// has already written `CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS` before
    /// dispatching the internal callback, so a re-delivery of the same
    /// `(chain_uid, sequence)` after a timeout is still rejected by the
    /// processed-sequence guard — no double-processing window opens up
    /// post-timeout.
    #[test]
    fn router_dedup_survives_packet_timeout() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();

        let tx = factory
            .register_denom(CrossChainConfig::default(), make_token("timeoutreorg"))
            .unwrap();

        // Advance the router-chain clock well past the packet's default
        // 60s timeout so the internal callback's timeout check trips.
        router_chain.app.borrow_mut().update_block(|block| {
            block.height += 1;
            block.time = block.time.plus_seconds(120);
        });

        // First delivery: outer `execute_receive_packet` commits the
        // sequence record; the inner callback then bails with PacketTimedOut.
        // The whole call still returns Ok at the relayer level — the ack
        // body just carries the failure.
        relay_factory_send_packet(tx.events.clone(), &router).unwrap();

        // Re-delivery must still be rejected by the sequence guard.
        let err = redeliver_factory_send_packet(tx.events, &router, "aftertimeout")
            .expect_err("router must reject re-delivery after timeout");
        assert!(
            format!("{err:?}").contains("Processed sequence already exists"),
            "expected sequence-dedup rejection after timeout, got: {err}"
        );
    }

    /// Scenario A "tx dropped" case: the source factory's tx is rolled back
    /// by a reorg, so its `PENDING_*` row never gets re-created — but the
    /// router has already processed the packet from the old block and sent
    /// the ack. The factory's ack handler MUST surface this as a clean
    /// error rather than silently mutate state or panic.
    ///
    /// Simulated here by manually deleting the factory's
    /// `PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS` row after the send (the
    /// equivalent of the reorg undoing the original `register_denom`'s
    /// state writes), then driving the ack through the full relayer path.
    #[test]
    fn factory_ack_errors_cleanly_when_pending_missing_post_reorg() {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, FACTORY_CHAIN_ID_IBC);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![FACTORY_CHAIN_ID_IBC]).unwrap();
        let factory =
            setup_factory(&interchain, FACTORY_CHAIN_ID_IBC, ROUTER_CHAIN_ID, &router).unwrap();
        let factory_chain_uid = factory.get_state().unwrap().chain_uid;

        let sender_addr = factory.environment().sender.clone();
        let tx = factory
            .register_denom(CrossChainConfig::default(), make_token("droppedtx"))
            .unwrap();

        // Identify the tx_id the factory generated for this call so we can
        // surgically remove its PENDING_* entry, mimicking the post-reorg
        // state where the factory has no record of the in-flight tx.
        let chain_uid_str = factory_chain_uid.as_str();
        let chain_id = factory.environment().app.borrow().block_info().chain_id;
        let predicted_tx_id = format!("{chain_uid_str}:{sender_addr}:{chain_id}:1");

        {
            let factory_addr = factory.address().unwrap();
            let mut app = factory.environment().app.borrow_mut();
            let mut storage = app.contract_storage_mut(&factory_addr);
            factory::state::PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS.remove(
                storage.as_mut(),
                (Addr::unchecked(sender_addr.as_str()), predicted_tx_id),
            );
        }

        // Relay the send to router so we get an ack to deliver back.
        let ack_events = relay_factory_send_packet(tx.events, &router).unwrap();

        // Now the ack lands on the factory with no matching PENDING_* entry.
        // The handler must surface this as a clean Err — no silent success,
        // no panic.
        let res = relay_factory_ack_packet(&factory, ack_events, &factory_chain_uid);
        assert!(
            res.is_err(),
            "expected factory ack to fail without matching PENDING_* row"
        );
    }
}
