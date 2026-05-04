#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response,
};
use rstest::rstest;

use super::utils::{last_position_id, sender, voucher_balance};
use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, collect_concentrated_fees, create_concentrated_pool,
    list_position_ids, remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::factory_register::FactorySetupMode;

const TICK_SPACING: i64 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[rstest]
    fn test_add_position_below_range_does_not_change_active_liquidity(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0_before: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Position entirely below current tick
        let lower = ((slot0_before.tick - 300) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0_before.tick - 100) / TICK_SPACING) * TICK_SPACING;
        assert!(
            upper <= slot0_before.tick,
            "position must be below current tick"
        );

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
            pool_key,
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let slot0_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert_eq!(
            slot0_after.liquidity, slot0_before.liquidity,
            "active liquidity must not change when adding out-of-range position below",
        );
    }

    #[rstest]
    fn test_add_position_above_range_does_not_change_active_liquidity(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0_before: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Position entirely above current tick
        let lower = ((slot0_before.tick + 100) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0_before.tick + 300) / TICK_SPACING) * TICK_SPACING;
        assert!(
            lower > slot0_before.tick,
            "position must be above current tick"
        );

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
            pool_key,
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let slot0_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert_eq!(
            slot0_after.liquidity, slot0_before.liquidity,
            "active liquidity must not change when adding out-of-range position above",
        );
    }

    #[rstest]
    fn test_out_of_range_position_earns_no_fees(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Add position far below current tick
        let lower = ((slot0.tick - 500) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0.tick - 300) / TICK_SPACING) * TICK_SPACING;

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 10_000, 10_000),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let oor_position_id = last_position_id(&factory);

        // Execute swaps that stay within the in-range position
        for _ in 0..3 {
            execute_concentrated_swap(
                &factory,
                &router,
                pool_key.clone(),
                token_a.clone(),
                token_b.token.clone(),
                Uint128::new(2_000),
            );
            execute_concentrated_swap(
                &factory,
                &router,
                pool_key.clone(),
                token_b.clone(),
                token_a.token.clone(),
                Uint128::new(2_000),
            );
        }

        // Out-of-range position should have zero tokens owed
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: oor_position_id,
            })
            .unwrap();
        assert_eq!(
            pos.tokens_owed_0,
            Uint128::zero(),
            "out-of-range position should have zero tokens_owed_0",
        );
        assert_eq!(
            pos.tokens_owed_1,
            Uint128::zero(),
            "out-of-range position should have zero tokens_owed_1",
        );

        // Global fee growth should have increased from the swaps
        let slot0_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert!(
            slot0_after.fee_growth_global_0_x128 > Uint256::zero()
                || slot0_after.fee_growth_global_1_x128 > Uint256::zero(),
            "swaps should have generated global fee growth",
        );
    }

    #[rstest]
    fn test_swap_into_out_of_range_position_activates_it(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        let active_liq_before = slot0.liquidity;

        // Wide OOR position below current tick so a moderate swap lands inside
        let lower = ((slot0.tick - 10_000) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0.tick - 10) / TICK_SPACING) * TICK_SPACING;

        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 20_000, 20_000),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();
        assert!(!add_resp.liquidity_delta.is_zero());

        let slot0_mid: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert_eq!(slot0_mid.liquidity, active_liq_before);

        // Small swap to nudge tick down past upper bound of OOR position
        execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_a.clone(),
            token_b.token.clone(),
            Uint128::new(5_000),
        );

        let slot0_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        assert!(
            slot0_after.tick < upper,
            "swap should push tick below OOR upper bound: tick={}, upper={}",
            slot0_after.tick,
            upper,
        );
        assert!(
            slot0_after.tick >= lower,
            "tick should remain above OOR lower bound: tick={}, lower={}",
            slot0_after.tick,
            lower,
        );
        assert!(
            slot0_after.liquidity > active_liq_before,
            "active liquidity should increase when tick enters out-of-range position: \
             before={}, after={}, tick={}",
            active_liq_before,
            slot0_after.liquidity,
            slot0_after.tick,
        );
    }

    #[rstest]
    fn test_remove_liquidity_from_out_of_range_position(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Add position above current tick
        let lower = ((slot0.tick + 100) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0.tick + 300) / TICK_SPACING) * TICK_SPACING;

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 10_000, 10_000),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let oor_id = last_position_id(&factory);
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: oor_id,
            })
            .unwrap();
        let liq = pos.liquidity;
        assert!(!liq.is_zero());

        let active_before: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Partial remove from out-of-range position
        let half = Uint128::new(liq.u128() / 2);
        remove_concentrated_liquidity(&factory, &router, pool_key.clone(), oor_id, half).unwrap();

        let active_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert_eq!(
            active_after.liquidity, active_before.liquidity,
            "removing from out-of-range position must not change active liquidity",
        );

        let pos_after: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: oor_id,
            })
            .unwrap();
        assert_eq!(
            pos_after.liquidity,
            liq - half,
            "position liquidity should decrease by removed amount",
        );

        // Full remove of remaining
        remove_concentrated_liquidity(&factory, &router, pool_key, oor_id, pos_after.liquidity)
            .unwrap();

        let pos_result: Result<PositionResponse, _> = vlp.query(&ConcentratedQueryMsg::Position {
            position_id: oor_id,
        });
        assert!(
            pos_result.is_err(),
            "out-of-range position should be deleted after full removal",
        );
    }

    #[rstest]
    fn test_add_to_existing_out_of_range_position_keeps_active_liquidity(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Create out-of-range position below
        let lower = ((slot0.tick - 400) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0.tick - 200) / TICK_SPACING) * TICK_SPACING;

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let oor_id = last_position_id(&factory);
        let pos_before: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: oor_id,
            })
            .unwrap();
        let active_before: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Add more to the same OOR position
        let add_resp = add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
            pool_key,
            lower,
            upper,
            Some(oor_id),
            10_000,
        )
        .unwrap();
        assert!(!add_resp.liquidity_delta.is_zero());

        let active_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert_eq!(
            active_after.liquidity, active_before.liquidity,
            "adding to existing out-of-range position must not change active liquidity",
        );

        let pos_after: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: oor_id,
            })
            .unwrap();
        assert_eq!(
            pos_after.liquidity,
            pos_before.liquidity + add_resp.liquidity_delta,
            "position liquidity should increase by add delta",
        );

        let ids = list_position_ids(&factory).unwrap();
        assert!(
            ids.iter().any(|id| id == &oor_id.to_string()),
            "position ID should remain the same after adding to existing",
        );
    }

    #[rstest]
    fn test_collect_fees_on_out_of_range_position_returns_zero(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Add position far above current tick
        let lower = ((slot0.tick + 200) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0.tick + 400) / TICK_SPACING) * TICK_SPACING;

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 10_000, 10_000),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let oor_id = last_position_id(&factory);

        // Swap back and forth (stays in range of initial position, never reaches OOR)
        for _ in 0..3 {
            execute_concentrated_swap(
                &factory,
                &router,
                pool_key.clone(),
                token_a.clone(),
                token_b.token.clone(),
                Uint128::new(3_000),
            );
            execute_concentrated_swap(
                &factory,
                &router,
                pool_key.clone(),
                token_b.clone(),
                token_a.token.clone(),
                Uint128::new(3_000),
            );
        }

        let before_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
        let before_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

        collect_concentrated_fees(&factory, &router, pool_key, oor_id, sender(&factory)).unwrap();

        let after_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
        let after_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

        assert_eq!(
            after_0, before_0,
            "out-of-range position should collect zero fees for token_a",
        );
        assert_eq!(
            after_1, before_1,
            "out-of-range position should collect zero fees for token_b",
        );
    }

    #[rstest]
    fn test_position_earns_fees_only_after_tick_enters_range(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.factory_chain_id();
        let (_interchain, factory, router, token_a, token_b) =
            setup_concentrated_env(mode, factory_chain_id);
        let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
        let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

        let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
        let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
        let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

        // Wide OOR position below current tick
        let lower = ((slot0.tick - 10_000) / TICK_SPACING) * TICK_SPACING;
        let upper = ((slot0.tick - 10) / TICK_SPACING) * TICK_SPACING;

        add_concentrated_liquidity(
            &factory,
            &router,
            pair_with_amounts(&token_a, &token_b, 20_000, 20_000),
            pool_key.clone(),
            lower,
            upper,
            None,
            10_000,
        )
        .unwrap();

        let oor_id = last_position_id(&factory);

        // Small swap in opposite direction — stays in range, OOR earns nothing
        execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_b.clone(),
            token_a.token.clone(),
            Uint128::new(1_000),
        );

        let pos_before_entry: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: oor_id,
            })
            .unwrap();
        assert_eq!(pos_before_entry.tokens_owed_0, Uint128::zero());
        assert_eq!(pos_before_entry.tokens_owed_1, Uint128::zero());

        // Controlled swap to push tick down into OOR range
        execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_a.clone(),
            token_b.token.clone(),
            Uint128::new(5_000),
        );

        let slot0_after_big: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
        assert!(
            slot0_after_big.tick >= lower && slot0_after_big.tick < upper,
            "tick should land inside OOR position range: tick={}, range=[{}, {})",
            slot0_after_big.tick,
            lower,
            upper,
        );

        // Swap within OOR position's range to generate fees for it
        execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_b.clone(),
            token_a.token.clone(),
            Uint128::new(2_000),
        );

        let before_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
        let before_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

        collect_concentrated_fees(&factory, &router, pool_key, oor_id, sender(&factory)).unwrap();

        let after_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
        let after_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

        assert!(
            after_0 > before_0 || after_1 > before_1,
            "position should earn fees after tick moves into its range",
        );
    }
}
