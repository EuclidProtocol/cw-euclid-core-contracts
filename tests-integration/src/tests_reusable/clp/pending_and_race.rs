#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Event, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{PositionResponse, QueryMsg as ConcentratedQueryMsg};
use euclid_ibc::ack::make_ack_fail;
use rstest::rstest;
use rstest_reuse::apply;

use super::utils::{raw_units, scaled_pair, setup_clp};
use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, faucet, get_position_token,
    list_position_ids, remove_concentrated_liquidity,
};
use crate::helpers::relayer::{
    extract_send_packet_events, relay_factory_ack_packet, relay_factory_send_packet,
};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::factory_register::FactorySetupMode;
use crate::tests_reusable::test_macros::clp_matrix;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_reusable::clp::utils::first_position_id;

    fn position_liquidity_from_vlp(
        _factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
        router: &router::RouterContract<cw_orch::mock::MockBase>,
        pool_key: &euclid::msgs::vlp::base::PoolKey,
        position_id: Uint128,
    ) -> Uint128 {
        let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();
        pos.liquidity
    }

    fn position_liquidity_from_token(
        factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
        position_id: Uint128,
    ) -> Uint128 {
        let position_token = get_position_token(factory).unwrap();
        let info: euclid::msgs::position_token::query::PositionInfoResponse = position_token
            .query(&euclid::msgs::position_token::QueryMsg::PositionInfo {
                token_id: position_id.to_string(),
            })
            .unwrap();
        info.liquidity
    }

    /// Send two add_concentrated_liquidity requests without relaying acks,
    /// then relay acks one at a time. Both positions should be minted,
    /// giving 3 total (initial + 2 new).
    #[rstest]
    fn test_add_liquidity_while_previous_add_pending(
        #[values((6, 6), (6, 18), (8, 6))] decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(FactorySetupMode::Ibc, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key =
            create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

        let ids_after_create = list_position_ids(&factory).unwrap();
        assert_eq!(
            ids_after_create.len(),
            1,
            "pool creation mints one position"
        );

        // First add: tick range [-240, -120), no relay of ack yet
        let pair1 = scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b);
        let mut funds1 = vec![];
        for token in pair1.get_vec_token_info() {
            faucet(
                factory.environment(),
                factory.environment().sender.as_str(),
                Uint128::try_from(token.amount).unwrap().u128(),
                token.token_type,
                &mut funds1,
            );
        }
        let tx1 = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::AddConcentratedLiquidity {
                    pair_with_denom_and_amount: pair1,
                    pool_key: pool_key.clone(),
                    lower_tick_index: -240,
                    upper_tick_index: -120,
                    position_id: None,
                    slippage_tolerance_bps: 10_000,
                    cross_chain_config: CrossChainConfig::default(),
                },
                &funds1,
            )
            .unwrap();
        let ack_events1 = relay_factory_send_packet(tx1.events, &router).unwrap();

        // Second add: tick range [120, 240), no relay of ack yet
        let pair2 = scaled_pair(&token_a, &token_b, 5, decimals_a, 5, decimals_b);
        let mut funds2 = vec![];
        for token in pair2.get_vec_token_info() {
            faucet(
                factory.environment(),
                factory.environment().sender.as_str(),
                Uint128::try_from(token.amount).unwrap().u128(),
                token.token_type,
                &mut funds2,
            );
        }
        let tx2 = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::AddConcentratedLiquidity {
                    pair_with_denom_and_amount: pair2,
                    pool_key: pool_key.clone(),
                    lower_tick_index: 120,
                    upper_tick_index: 240,
                    position_id: None,
                    slippage_tolerance_bps: 10_000,
                    cross_chain_config: CrossChainConfig::default(),
                },
                &funds2,
            )
            .unwrap();
        let ack_events2 = relay_factory_send_packet(tx2.events, &router).unwrap();

        // Before any ack delivery, factory should still have only 1 position
        let ids_before_ack = list_position_ids(&factory).unwrap();
        assert_eq!(
            ids_before_ack.len(),
            1,
            "no new positions should appear before ack delivery"
        );

        // Deliver first ack
        let chain_uid = factory.get_state().unwrap().chain_uid;
        relay_factory_ack_packet(&factory, ack_events1, &chain_uid).unwrap();
        let ids_after_first_ack = list_position_ids(&factory).unwrap();
        assert_eq!(
            ids_after_first_ack.len(),
            2,
            "first ack should mint one new position"
        );

        // Deliver second ack
        relay_factory_ack_packet(&factory, ack_events2, &chain_uid).unwrap();
        let ids_after_second_ack = list_position_ids(&factory).unwrap();
        assert_eq!(
            ids_after_second_ack.len(),
            3,
            "second ack should mint another position, total 3"
        );
    }

    /// Remove half liquidity from a position, then add back to the same position.
    /// Verify the position still exists with adjusted liquidity.
    #[apply(clp_matrix)]
    fn test_remove_then_add_same_position_sequential(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key =
            create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

        let position_id = first_position_id(&factory);
        let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
        let initial_pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();
        let original_liquidity = initial_pos.liquidity;

        // Remove half
        let remove_amount = Uint128::new(original_liquidity.u128() / 2);
        remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key.clone(),
            position_id,
            remove_amount,
        )
        .unwrap();

        let after_remove_liquidity =
            position_liquidity_from_vlp(&factory, &router, &pool_key, position_id);
        assert_eq!(
            after_remove_liquidity,
            original_liquidity - remove_amount,
            "liquidity after remove should equal original - removed",
        );

        // Add back to same position with same tick range
        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b),
            pool_key.clone(),
            initial_pos.lower_tick_index,
            initial_pos.upper_tick_index,
            Some(position_id),
            10_000,
        )
        .unwrap();
        assert!(
            !add_resp.liquidity_delta.is_zero(),
            "add should produce non-zero liquidity delta"
        );

        // Verify position still exists with same ID
        let ids = list_position_ids(&factory).unwrap();
        assert_eq!(ids.len(), 1, "should still be exactly one position");
        assert_eq!(ids[0], position_id.to_string());

        // Verify exact liquidity using delta from add response
        let expected_final = after_remove_liquidity + add_resp.liquidity_delta;
        let final_liquidity =
            position_liquidity_from_vlp(&factory, &router, &pool_key, position_id);
        assert_eq!(
            final_liquidity, expected_final,
            "final VLP liquidity should equal post-remove + add delta from response",
        );
        // Cross-check: position token must agree with VLP
        let token_liquidity = position_liquidity_from_token(&factory, position_id);
        assert_eq!(
            token_liquidity, final_liquidity,
            "position token liquidity must match VLP after add-back",
        );
    }

    /// Remove half liquidity, then try to remove the original full amount.
    /// The second remove should fail because remaining < requested.
    #[apply(clp_matrix)]
    fn test_double_remove_same_position_second_fails(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let position_id = first_position_id(&factory);
        let original_liquidity =
            position_liquidity_from_vlp(&factory, &router, &pool_key, position_id);

        // Remove half (succeeds)
        let half = Uint128::new(original_liquidity.u128() / 2);
        remove_concentrated_liquidity(&factory, &router, pool_key.clone(), position_id, half)
            .unwrap();

        // Try to remove the original full amount (more than remaining)
        let err = remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key.clone(),
            position_id,
            original_liquidity,
        )
        .unwrap_err();
        assert!(
            !err.to_string().is_empty(),
            "removing more than remaining liquidity should fail"
        );

        // Position should still exist with reduced liquidity
        let remaining = position_liquidity_from_vlp(&factory, &router, &pool_key, position_id);
        let expected_remaining = original_liquidity - half;
        assert_eq!(
            remaining, expected_remaining,
            "position should retain exactly original - half liquidity"
        );

        let ids = list_position_ids(&factory).unwrap();
        assert_eq!(
            ids.len(),
            1,
            "position NFT should still exist after failed second remove"
        );
        assert_eq!(
            ids[0],
            position_id.to_string(),
            "remaining position should have the original ID"
        );
    }

    /// Send a remove request via IBC, then deliver a synthetic error ack.
    /// The position token liquidity (pre-decremented on send) must be
    /// restored to its original value.
    #[rstest]
    fn test_remove_ack_failure_restores_position_liquidity(
        #[values((6, 6), (6, 18), (8, 6))] decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(FactorySetupMode::Ibc, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let position_id = first_position_id(&factory);
        let original_liquidity = position_liquidity_from_token(&factory, position_id);
        assert!(
            original_liquidity > Uint128::zero(),
            "position should have non-zero liquidity"
        );

        // Send remove request (only factory->router, don't deliver real ack)
        let half = Uint128::new(original_liquidity.u128() / 2);
        let chain_uid = factory.get_state().unwrap().chain_uid;
        let sender =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        let tx = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::RemoveConcentratedLiquidity {
                    pool_key: pool_key.clone(),
                    position_id,
                    liquidity_delta: half,
                    recipient: sender,
                    cross_chain_config: CrossChainConfig::default(),
                },
                &[],
            )
            .unwrap();

        // Position token liquidity should be pre-decremented
        let after_send_liquidity = position_liquidity_from_token(&factory, position_id);
        assert_eq!(
            after_send_liquidity,
            original_liquidity - half,
            "position token liquidity should be pre-decremented after send"
        );

        // Construct synthetic error ack
        let send_packets = extract_send_packet_events(&tx.events);
        assert!(
            !send_packets.is_empty(),
            "expected at least one send packet"
        );

        let error_ack = make_ack_fail("simulated router error".to_string()).unwrap();
        let ack_event_type = format!("wasm-{}", EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT);
        let mut fake_ack_events = vec![];
        for packet in &send_packets {
            fake_ack_events.push(
                Event::new(&ack_event_type)
                    .add_attribute("msg", packet.msg.to_base64())
                    .add_attribute("sequence", packet.sequence.to_string())
                    .add_attribute("source_port", &packet.destination_port)
                    .add_attribute("destination_port", &packet.source_port),
            );
            fake_ack_events
                .push(Event::new(&ack_event_type).add_attribute("ack", error_ack.to_base64()));
        }

        // Deliver error ack to factory
        relay_factory_ack_packet(&factory, fake_ack_events, &chain_uid).unwrap();

        // Position token liquidity must be restored
        let restored_liquidity = position_liquidity_from_token(&factory, position_id);
        assert_eq!(
            restored_liquidity, original_liquidity,
            "error ack must restore position token liquidity to original value"
        );
    }

    /// Create pool + position, execute swaps to accrue fees, then send a
    /// partial remove (only relay send, not ack). While the remove is
    /// "in flight", attempt to collect fees.
    #[rstest]
    fn test_collect_fees_during_remove_pending(
        #[values((6, 6), (6, 18), (8, 6))] decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(FactorySetupMode::Ibc, decimals_a, decimals_b);
        let pair = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let position_id = first_position_id(&factory);
        let original_liquidity =
            position_liquidity_from_vlp(&factory, &router, &pool_key, position_id);

        // Execute swaps to accrue fees (keep iteration count low to stay
        // within the IBC rate limit of 10 pending packets)
        for _ in 0..3 {
            execute_concentrated_swap(
                &factory,
                &router,
                pool_key.clone(),
                token_a.clone(),
                token_b.token.clone(),
                Uint256::from(raw_units(1, decimals_a)),
            );
            execute_concentrated_swap(
                &factory,
                &router,
                pool_key.clone(),
                token_b.clone(),
                token_a.token.clone(),
                Uint256::from(raw_units(1, decimals_b)),
            );
        }

        // Send partial remove (half liquidity), only relay send, not ack
        let half = Uint128::new(original_liquidity.u128() / 2);
        let chain_uid = factory.get_state().unwrap().chain_uid;
        let sender =
            CrossChainUser::new(chain_uid.clone(), factory.environment().sender.to_string());
        let tx = factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::RemoveConcentratedLiquidity {
                    pool_key: pool_key.clone(),
                    position_id,
                    liquidity_delta: half,
                    recipient: sender.clone(),
                    cross_chain_config: CrossChainConfig::default(),
                },
                &[],
            )
            .unwrap();
        let _ack_events = relay_factory_send_packet(tx.events, &router).unwrap();

        // While remove is "in flight" (ack not yet delivered to factory),
        // attempt to collect fees. The collect goes through factory which
        // also does IBC, so we execute it directly and relay.
        let collect_result = factory.execute(
            &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
                pool_key: pool_key.clone(),
                position_id,
                recipient: sender,
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        );

        // Document actual behavior: collect fees may succeed or fail while
        // a remove is pending. Either outcome is acceptable as long as the
        // system remains consistent.
        match collect_result {
            Ok(collect_tx) => {
                // If it succeeds on factory side, try to relay
                let relay_result = relay_factory_send_packet(collect_tx.events, &router);
                // Whether relay succeeds or not, the system should remain consistent
                if let Ok(collect_ack_events) = relay_result {
                    let _ = relay_factory_ack_packet(&factory, collect_ack_events, &chain_uid);
                }
            }
            Err(_) => {
                // Collecting fees during pending remove was rejected at factory level.
                // This is acceptable behavior.
            }
        }

        // Verify the position still exists (the remove ack was never delivered)
        let ids = list_position_ids(&factory).unwrap();
        assert_eq!(ids.len(), 1, "exactly one position should exist");
        assert_eq!(
            ids[0],
            position_id.to_string(),
            "remaining position should have the original ID"
        );
    }
}
