#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Addr;
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{PositionResponse, QueryMsg as ConcentratedQueryMsg};
use rstest::rstest;
use rstest_reuse::apply;

use super::utils::{first_position_id, scaled_pair, setup_clp};
use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, get_position_token, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::factory_register::FactorySetupMode;
use crate::tests_reusable::test_macros::clp_matrix;

#[cfg(test)]
mod tests {
    use super::*;

    #[apply(clp_matrix)]
    fn test_position_ids_are_sequential(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        // Pool creation mints position #1
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        // Add a second position at different ticks -> ID should be 2
        let pair2 = scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b);
        add_concentrated_liquidity(
            &factory,
            &router,
            pair2,
            pool_key.clone(),
            -240,
            -120,
            None,
            10_000,
        )
        .unwrap();

        // Add a third position at yet another range -> ID should be 3
        let pair3 = scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b);
        add_concentrated_liquidity(
            &factory,
            &router,
            pair3,
            pool_key.clone(),
            120,
            240,
            None,
            10_000,
        )
        .unwrap();

        let ids = list_position_ids(&factory).unwrap();
        assert_eq!(ids.len(), 3, "should have exactly 3 positions");

        let parsed: Vec<u128> = ids.iter().map(|id| id.parse::<u128>().unwrap()).collect();
        for i in 1..parsed.len() {
            assert!(
                parsed[i] > parsed[i - 1],
                "position IDs must be strictly increasing: {} should be > {}",
                parsed[i],
                parsed[i - 1],
            );
        }

        // First position should be ID 1
        assert_eq!(parsed[0], 1, "first position ID should be 1");
        assert_eq!(parsed[1], 2, "second position ID should be 2");
        assert_eq!(parsed[2], 3, "third position ID should be 3");
    }

    #[apply(clp_matrix)]
    fn test_position_id_maps_to_correct_vlp(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        // Create two pools with different fee tiers
        let pair_500 = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key_500 =
            create_concentrated_pool(&factory, &router, pair_500, 500, 10, 100).unwrap();

        let pair_3000 = scaled_pair(&token_a, &token_b, 20, decimals_a, 20, decimals_b);
        let pool_key_3000 =
            create_concentrated_pool(&factory, &router, pair_3000, 3_000, 60, 100).unwrap();

        // Each pool creation mints one position
        let ids = list_position_ids(&factory).unwrap();
        assert_eq!(ids.len(), 2, "two pools should produce two positions");

        let position_token = get_position_token(&factory).unwrap();

        // Query position info for each position
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

        // Query router for VLP addresses by pool key
        let vlp_500 = router.get_vlp_by_pool_key(pool_key_500).unwrap().vlp;
        let vlp_3000 = router.get_vlp_by_pool_key(pool_key_3000).unwrap().vlp;

        // Position IDs are sequential: id[0]=1 from pool 500, id[1]=2 from pool 3000
        assert_eq!(
            info_0.vlp_address, vlp_500,
            "first position should map to the 500bps pool VLP",
        );
        assert_eq!(
            info_1.vlp_address, vlp_3000,
            "second position should map to the 3000bps pool VLP",
        );

        // Positions from different pools must point to different VLPs
        assert_ne!(
            info_0.vlp_address, info_1.vlp_address,
            "positions from different fee tiers must point to different VLPs",
        );
    }

    #[apply(clp_matrix)]
    fn test_burned_position_id_not_reused(mode: FactorySetupMode, decimal_pair: (u32, u32)) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        // Pool creation mints the initial position
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let initial_id = first_position_id(&factory);

        // Query position's liquidity from VLP to get exact amount for full removal
        let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(&vlp_address));
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: initial_id,
            })
            .unwrap();

        // Fully remove the position to burn it
        remove_concentrated_liquidity(
            &factory,
            &router,
            pool_key.clone(),
            initial_id,
            pos.liquidity,
        )
        .unwrap();

        // Create a new position at different ticks
        let pair2 = scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b);
        add_concentrated_liquidity(
            &factory,
            &router,
            pair2,
            pool_key.clone(),
            -240,
            -120,
            None,
            10_000,
        )
        .unwrap();

        let ids = list_position_ids(&factory).unwrap();
        let parsed: Vec<u128> = ids.iter().map(|id| id.parse::<u128>().unwrap()).collect();

        // The new position should have ID = initial_id + 1, not reuse the burned ID
        assert_eq!(
            parsed.len(),
            1,
            "should have exactly one position after burn + create"
        );
        let new_id = parsed[0];
        assert_eq!(
            new_id,
            initial_id.u128() + 1,
            "new position ID should be exactly initial_id + 1, IDs are never recycled",
        );
    }

    #[apply(clp_matrix)]
    fn test_position_id_consistent_across_add_operations(
        mode: FactorySetupMode,
        decimal_pair: (u32, u32),
    ) {
        let (decimals_a, decimals_b) = decimal_pair;
        let (_interchain, factory, router, token_a, token_b) =
            setup_clp(mode, decimals_a, decimals_b);

        // Pool creation mints position #1
        let pair = scaled_pair(&token_a, &token_b, 30, decimals_a, 30, decimals_b);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let position_id = first_position_id(&factory);
        let ids_before = list_position_ids(&factory).unwrap();

        // Get the position's tick range from VLP
        let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
        let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(&vlp_address));
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id })
            .unwrap();

        // Add more liquidity to the same position using Some(position_id) and same tick range
        let pair2 = scaled_pair(&token_a, &token_b, 10, decimals_a, 10, decimals_b);
        add_concentrated_liquidity(
            &factory,
            &router,
            pair2,
            pool_key.clone(),
            pos.lower_tick_index,
            pos.upper_tick_index,
            Some(position_id),
            100,
        )
        .unwrap();

        let ids_after = list_position_ids(&factory).unwrap();

        // Should still have the same number of positions
        assert_eq!(
            ids_before.len(),
            ids_after.len(),
            "adding to existing position should not create a new position ID",
        );

        // The position ID list should still contain exactly the original ID
        assert_eq!(
            ids_after,
            vec![position_id.to_string()],
            "position list should contain exactly the original position ID after adding liquidity",
        );
    }
}
