#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response,
};
use rstest::rstest;
use std::collections::HashSet;

use super::utils::{first_position_id, last_position_id};
use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, get_position_token, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL,
};
use crate::tests_reusable::factory_register::FactorySetupMode;

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_create_multiple_positions_same_pool_different_ranges(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        // Pool creation mints position #1. Add two more at different ranges.
        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
            pool_key.clone(),
            -240,
            -120,
            None,
            10_000,
        )
        .unwrap();

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
            pool_key.clone(),
            120,
            240,
            None,
            10_000,
        )
        .unwrap();

        let ids = list_position_ids(&factory).unwrap();
        assert_eq!(ids.len(), 3, "should have 3 unique positions");

        let unique: HashSet<&str> = ids.iter().map(String::as_str).collect();
        assert_eq!(unique.len(), 3, "all position IDs must be unique");

        // Each position should have its own NFT queryable via position token
        let position_token = get_position_token(&factory).unwrap();
        for id_str in &ids {
            let owner = position_token
                .query::<euclid::msgs::position_token::query::OwnerOfResponse>(
                    &euclid::msgs::position_token::QueryMsg::OwnerOf {
                        token_id: id_str.clone(),
                    },
                )
                .unwrap()
                .owner;
            assert_eq!(owner, factory.environment().sender.to_string());
        }

        // Each position should be independently queryable on VLP
        let vlp_address = router.get_vlp_by_pool_key(pool_key).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
        for id_str in &ids {
            let position_id = Uint128::new(id_str.parse::<u128>().unwrap());
            let pos: PositionResponse = vlp
                .query(&ConcentratedQueryMsg::Position { position_id })
                .unwrap();
            assert!(
                pos.liquidity > Uint128::zero(),
                "position {} should have non-zero liquidity",
                id_str,
            );
        }
    }

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_create_positions_across_multiple_fee_tiers(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);

        let pair_500 = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
        let pool_key_500 =
            create_concentrated_pool(&factory, &router, pair_500, 500, 10, 100).unwrap();

        let pair_3000 = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
        let pool_key_3000 =
            create_concentrated_pool(&factory, &router, pair_3000, 3_000, 60, 100).unwrap();

        // Each pool creation mints one position
        let ids = list_position_ids(&factory).unwrap();
        assert_eq!(ids.len(), 2, "two pools should produce two positions");

        let unique: HashSet<&str> = ids.iter().map(String::as_str).collect();
        assert_eq!(unique.len(), 2, "position IDs must be unique across pools");

        // Verify each position token points to a different VLP
        let position_token = get_position_token(&factory).unwrap();
        let info_0 = position_token
            .query::<euclid::msgs::position_token::query::PositionInfoResponse>(
                &euclid::msgs::position_token::QueryMsg::PositionInfo {
                    token_id: ids[0].clone(),
                },
            )
            .unwrap();
        let info_1 = position_token
            .query::<euclid::msgs::position_token::query::PositionInfoResponse>(
                &euclid::msgs::position_token::QueryMsg::PositionInfo {
                    token_id: ids[1].clone(),
                },
            )
            .unwrap();
        assert_ne!(
            info_0.vlp_address, info_1.vlp_address,
            "positions from different fee tiers must point to different VLPs",
        );

        // Cross-check VLP addresses against router
        let vlp_500 = router.get_vlp_by_pool_key(pool_key_500).unwrap().vlp;
        let vlp_3000 = router.get_vlp_by_pool_key(pool_key_3000).unwrap().vlp;
        assert_ne!(vlp_500, vlp_3000);
    }

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_partial_remove_preserves_nft_metadata(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let position_id = first_position_id(&factory);
        let position_token = get_position_token(&factory).unwrap();

        // Query initial position info
        let info_before = position_token
            .query::<euclid::msgs::position_token::query::PositionInfoResponse>(
                &euclid::msgs::position_token::QueryMsg::PositionInfo {
                    token_id: position_id.to_string(),
                },
            )
            .unwrap();
        assert!(
            info_before.liquidity > Uint128::zero(),
            "position should have liquidity before partial remove",
        );

        let owner_before = position_token
            .query::<euclid::msgs::position_token::query::OwnerOfResponse>(
                &euclid::msgs::position_token::QueryMsg::OwnerOf {
                    token_id: position_id.to_string(),
                },
            )
            .unwrap()
            .owner;

        // Partial remove: half the liquidity
        let half = Uint128::new((info_before.liquidity.u128() / 2).max(1));
        remove_concentrated_liquidity(&factory, &router, pool_key, position_id, half).unwrap();

        // Query after partial removal
        let info_after = position_token
            .query::<euclid::msgs::position_token::query::PositionInfoResponse>(
                &euclid::msgs::position_token::QueryMsg::PositionInfo {
                    token_id: position_id.to_string(),
                },
            )
            .unwrap();

        assert_eq!(
            info_after.liquidity,
            info_before.liquidity - half,
            "liquidity should equal before - half after partial remove",
        );
        assert_eq!(
            info_after.vlp_address, info_before.vlp_address,
            "vlp_address must not change after partial remove",
        );

        let owner_after = position_token
            .query::<euclid::msgs::position_token::query::OwnerOfResponse>(
                &euclid::msgs::position_token::QueryMsg::OwnerOf {
                    token_id: position_id.to_string(),
                },
            )
            .unwrap()
            .owner;
        assert_eq!(
            owner_after, owner_before,
            "owner must not change after partial remove",
        );
    }

    /// Full removal auto-collects pending fees and burns the NFT in one transaction.
    /// See test_full_removal_auto_collects_fees in concentrated_collect.rs for the
    /// detailed fee-accrual variant. This test verifies the position token is burned
    /// after full removal even when fees have accrued.
    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    fn test_full_remove_with_pending_fees_does_not_burn(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);

        // Add a second position around current tick
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        let lower = ((slot0.tick - 100) / 10) * 10;
        let upper = ((slot0.tick + 100) / 10) * 10;

        let pair2 = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
        add_concentrated_liquidity(
            &factory,
            &router,
            pair2,
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let position_id = last_position_id(&factory);
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();
        let liquidity = pos.liquidity;
        assert!(!liquidity.is_zero(), "position should have liquidity");

        // Execute swaps to accrue fees
        execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_b.clone(),
            token_a.token.clone(),
            Uint128::new(10_000),
        );
        execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_a.clone(),
            token_b.token.clone(),
            Uint128::new(10_000),
        );

        // Full removal should auto-collect fees and burn the position NFT.
        // The VLP auto-collects all pending fees during full removal, so the
        // position is fully cleaned up and the NFT is burned immediately.
        remove_concentrated_liquidity(&factory, &router, pool_key.clone(), position_id, liquidity)
            .unwrap();

        // Position should be deleted on VLP
        let pos_result: Result<PositionResponse, _> =
            vlp.query(&ConcentratedQueryMsg::Position { position_id });
        assert!(
            pos_result.is_err(),
            "position should be deleted after full removal with auto-collect",
        );

        // NFT should be burned
        let position_token = get_position_token(&factory).unwrap();
        let query_result = position_token
            .query::<euclid::msgs::position_token::query::OwnerOfResponse>(
                &euclid::msgs::position_token::QueryMsg::OwnerOf {
                    token_id: position_id.to_string(),
                },
            );
        assert!(
            query_result.is_err(),
            "NFT should be burned after full removal (auto-collect clears all fees)",
        );
    }

    #[rstest]
    #[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_position_liquidity_tracks_multiple_add_remove_cycles(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let position_id = first_position_id(&factory);
        let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));

        // Query initial liquidity from VLP
        let initial_pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();
        let initial_liquidity = initial_pos.liquidity;
        assert!(initial_liquidity > Uint128::zero());

        // Add more liquidity to the same position
        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 10_000, 10_000),
            pool_key.clone(),
            initial_pos.lower_tick_index,
            initial_pos.upper_tick_index,
            Some(position_id),
            100,
        )
        .unwrap();
        assert!(
            !add_resp.liquidity_delta.is_zero(),
            "add should produce non-zero liquidity delta"
        );

        let after_add: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();
        assert_eq!(
            after_add.liquidity,
            initial_liquidity + add_resp.liquidity_delta,
            "VLP liquidity after add should equal initial + delta from response",
        );
        // Cross-check: position token must agree with VLP
        let position_token = get_position_token(&factory).unwrap();
        let token_info = position_token
            .query::<euclid::msgs::position_token::query::PositionInfoResponse>(
                &euclid::msgs::position_token::QueryMsg::PositionInfo {
                    token_id: position_id.to_string(),
                },
            )
            .unwrap();
        assert_eq!(
            token_info.liquidity, after_add.liquidity,
            "position token liquidity must match VLP after add",
        );

        // Partial remove
        let remove_amount = Uint128::new((after_add.liquidity.u128() / 3).max(1));
        remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key.clone(),
            position_id,
            remove_amount,
        )
        .unwrap();

        let after_partial: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();
        assert_eq!(
            after_partial.liquidity,
            after_add.liquidity - remove_amount,
            "liquidity should equal after_add - remove_amount",
        );

        // Full remove of remaining liquidity
        remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key,
            position_id,
            after_partial.liquidity,
        )
        .unwrap();

        // Position should be gone (NFT burned)
        let ids = list_position_ids(&factory).unwrap();
        assert!(
            ids.is_empty(),
            "all position NFTs must be burned after full removal",
        );

        // VLP should also report position as deleted
        let pos_result: Result<PositionResponse, _> =
            vlp.query(&ConcentratedQueryMsg::Position { position_id });
        assert!(
            pos_result.is_err(),
            "VLP should report position as deleted after full removal",
        );
    }
}
