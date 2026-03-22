#![cfg(not(target_arch = "wasm32"))]

//! Regression test for tick boundary rounding desync.
//!
//! After a swap crosses tick T going down, the swap loop sets slot0.tick = T-1
//! and adjusts ACTIVE_LIQUIDITY. If the next step barely moves the price,
//! get_tick_at_sqrt_ratio can round back to tick T, desyncing slot0.tick from
//! ACTIVE_LIQUIDITY. The fix clamps against the last crossed tick.

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response,
};
use rstest::rstest;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::FACTORY_CHAIN_ID_LOCAL;
use crate::tests_reusable::factory_register::FactorySetupMode;

/// After many small swaps crossing a position boundary, ACTIVE_LIQUIDITY
/// must still match the sum of in-range positions. Without the tick boundary
/// rounding fix, the price can round back to a just-crossed tick, desyncing
/// the liquidity state.
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
fn test_active_liquidity_consistent_after_boundary_crossings(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    // Small initial amounts → low liquidity → price moves easily
    let pair = pair_with_amounts(&token_a, &token_b, 1_000, 1_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);

    // Add a narrow position around current tick
    let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    let lower = ((slot0.tick - 50) / 10) * 10;
    let upper = ((slot0.tick + 50) / 10) * 10;

    let pair2 = pair_with_amounts(&token_a, &token_b, 500, 500);
    add_concentrated_liquidity(
        &factory, &router, pair2, pool_key.clone(),
        lower, upper, None, 10_000,
    )
    .expect("add narrow position should succeed");

    // Many small swaps back and forth to repeatedly cross the position boundary
    for _ in 0..10 {
        let _ = execute_concentrated_swap(
            &factory, &router, pool_key.clone(),
            token_b.clone(), token_a.token.clone(), Uint128::new(300),
        );
        let _ = execute_concentrated_swap(
            &factory, &router, pool_key.clone(),
            token_a.clone(), token_b.token.clone(), Uint128::new(300),
        );
    }

    // Verify ACTIVE_LIQUIDITY matches sum of in-range positions
    let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    let position_ids = list_position_ids(&factory).unwrap();

    let mut in_range_liquidity: u128 = 0;
    for id_str in &position_ids {
        let id = Uint128::new(id_str.parse::<u128>().unwrap());
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id: id })
            .unwrap();
        if pos.lower_tick_index <= slot0.tick && slot0.tick < pos.upper_tick_index {
            in_range_liquidity += pos.liquidity.u128();
        }
    }

    // Allow 1-tick boundary tolerance (V3 sets tick = crossed_tick - 1
    // even if price hasn't moved below that tick's sqrt_ratio)
    let price_tick = concentrated_vlp::math::tick_math::get_tick_at_sqrt_ratio(
        slot0.sqrt_price_x96,
    )
    .unwrap();
    let mut in_range_by_price: u128 = 0;
    for id_str in &position_ids {
        let id = Uint128::new(id_str.parse::<u128>().unwrap());
        let pos: PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position { position_id: id })
            .unwrap();
        if pos.lower_tick_index <= price_tick && price_tick < pos.upper_tick_index {
            in_range_by_price += pos.liquidity.u128();
        }
    }

    assert!(
        slot0.liquidity.u128() == in_range_liquidity
            || slot0.liquidity.u128() == in_range_by_price,
        "ACTIVE_LIQUIDITY ({}) should match in-range positions \
         by tick ({}, tick={}) or by price ({}, price_tick={})",
        slot0.liquidity, in_range_liquidity, slot0.tick,
        in_range_by_price, price_tick,
    );
}

/// Full removal should auto-collect pending fees and delete the position
/// in a single transaction. Before the fix, users needed a separate
/// collect_fees call to clear tokens_owed.
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
fn test_full_removal_auto_collects_fees(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);

    // Add a position
    let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    let lower = ((slot0.tick - 100) / 10) * 10;
    let upper = ((slot0.tick + 100) / 10) * 10;

    let pair2 = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    add_concentrated_liquidity(
        &factory, &router, pair2, pool_key.clone(),
        lower, upper, None, 10_000,
    )
    .expect("add should succeed");

    let position_id = {
        let ids = list_position_ids(&factory).unwrap();
        Uint128::new(ids.last().unwrap().parse::<u128>().unwrap())
    };
    let pos = vlp
        .query::<PositionResponse>(&ConcentratedQueryMsg::Position { position_id })
        .unwrap();
    let liquidity = pos.liquidity;
    assert!(!liquidity.is_zero(), "position should have liquidity");

    // Swap to accrue fees
    execute_concentrated_swap(
        &factory, &router, pool_key.clone(),
        token_b.clone(), token_a.token.clone(), Uint128::new(10_000),
    );
    execute_concentrated_swap(
        &factory, &router, pool_key.clone(),
        token_a.clone(), token_b.token.clone(), Uint128::new(10_000),
    );

    // Verify fees have accrued (fee_growth > 0)
    let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    assert!(
        slot0.fee_growth_global_0_x128 > cosmwasm_std::Uint256::zero()
            || slot0.fee_growth_global_1_x128 > cosmwasm_std::Uint256::zero(),
        "fees should have accrued from swaps"
    );

    // Full removal — should auto-collect fees and delete position
    remove_concentrated_liquidity(
        &factory, &router, pool_key.clone(), position_id, liquidity,
    )
    .expect("full removal should succeed");

    // Position should be fully deleted (not just zero liquidity)
    let pos_result: Result<PositionResponse, _> =
        vlp.query(&ConcentratedQueryMsg::Position { position_id });
    assert!(
        pos_result.is_err(),
        "position should be deleted after full removal with auto-collect, \
         but query succeeded with: {:?}",
        pos_result.ok()
    );
}
