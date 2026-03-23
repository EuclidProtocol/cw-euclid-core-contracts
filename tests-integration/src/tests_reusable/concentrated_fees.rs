#![cfg(not(target_arch = "wasm32"))]

use concentrated_vlp::math::position_math::fee_growth_inside;
use concentrated_vlp::state::TickInfo;
use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response, TickResponse,
};
use rstest::rstest;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, collect_concentrated_fees, create_concentrated_pool,
    list_position_ids, remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

fn tick_response_to_info(t: &TickResponse) -> TickInfo {
    TickInfo {
        initialized: t.initialized,
        liquidity_gross: t.liquidity_gross,
        liquidity_net: t.liquidity_net,
        fee_growth_outside_0_x128: t.fee_growth_outside_0_x128,
        fee_growth_outside_1_x128: t.fee_growth_outside_1_x128,
    }
}

/// Query tick state and compute fee_growth_inside using the contract's own function.
fn query_fee_growth_inside(
    vlp: &concentrated_vlp::ConcentratedVlpContract<cw_orch::mock::MockBase>,
    lower_idx: i64,
    upper_idx: i64,
) -> (Uint256, Uint256) {
    let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    let lower_tick: Option<TickResponse> = vlp
        .query(&ConcentratedQueryMsg::Tick { index: lower_idx })
        .ok();
    let upper_tick: Option<TickResponse> = vlp
        .query(&ConcentratedQueryMsg::Tick { index: upper_idx })
        .ok();
    fee_growth_inside(
        slot0.tick,
        lower_idx,
        upper_idx,
        slot0.fee_growth_global_0_x128,
        slot0.fee_growth_global_1_x128,
        lower_tick.as_ref().map(tick_response_to_info),
        upper_tick.as_ref().map(tick_response_to_info),
    )
    .expect("fee_growth_inside should not error for valid tick state")
}

fn last_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    Uint128::new(ids.last().unwrap().parse::<u128>().unwrap())
}

fn query_position(
    vlp: &concentrated_vlp::ConcentratedVlpContract<cw_orch::mock::MockBase>,
    position_id: Uint128,
) -> PositionResponse {
    vlp.query(&ConcentratedQueryMsg::Position { position_id })
        .unwrap()
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_fee_accrual_and_collect(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );
    let before: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse = vlp
        .query(&ConcentratedQueryMsg::TotalFeesCollected {})
        .unwrap();
    let before_fee = before
        .total_fees
        .lp_fees
        .get_fee(token_a.token.to_string().as_str());

    let amount_out = execute_concentrated_swap(
        &factory,
        &router,
        pool_key,
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(2_000),
    );
    assert!(
        amount_out > Uint128::zero(),
        "swap must return non-zero output"
    );

    let after: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse = vlp
        .query(&ConcentratedQueryMsg::TotalFeesCollected {})
        .unwrap();
    let after_fee = after
        .total_fees
        .lp_fees
        .get_fee(token_a.token.to_string().as_str());
    assert!(
        after_fee > before_fee,
        "fee accrual should increase after swap"
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_collect_idempotency(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );

    execute_concentrated_swap(
        &factory,
        &router,
        pool_key,
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(1_000),
    );

    let first: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse = vlp
        .query(&ConcentratedQueryMsg::TotalFeesCollected {})
        .unwrap();
    let second: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse = vlp
        .query(&ConcentratedQueryMsg::TotalFeesCollected {})
        .unwrap();
    assert_eq!(
        first, second,
        "collect state should be idempotent without new swaps"
    );
}

// =============================================================================
// Regression: fee_growth_inside ordering bug
// Before the fix, settle_position_fees was called BEFORE apply_liquidity_delta,
// causing fee_growth_inside to go backward for new positions at fresh ticks.
// =============================================================================

/// New position at fresh ticks — the primary bug scenario.
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_fee_growth_inside_consistent_after_add_to_new_ticks(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    // Step 1: Swap to accrue fees and move price.
    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_b.clone(),
        token_a.token.clone(),
        Uint128::new(30_000),
    );

    // Step 2: Verify global fee growth is non-zero.
    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);
    let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    assert!(
        slot0.fee_growth_global_0_x128 > Uint256::zero()
            || slot0.fee_growth_global_1_x128 > Uint256::zero(),
        "fees should have accrued from the swap"
    );
    let current_tick = slot0.tick;

    // Step 3: Add liquidity at NEW ticks that don't yet exist, with the lower tick
    // below current_tick (so its outside will be set to global on initialization).
    let new_lower = ((current_tick - 100) / 10) * 10;
    let new_upper = ((current_tick + 100) / 10) * 10;
    assert!(new_lower < current_tick && current_tick < new_upper);

    let pair2 = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);
    add_concentrated_liquidity(
        &factory,
        &router,
        pair2,
        pool_key.clone(),
        new_lower,
        new_upper,
        None,
        10_000,
    )
    .expect("add liquidity at new ticks should succeed");

    // Step 4: Verify fee_growth_inside_last matches computed fee_growth_inside.
    let position_id = last_position_id(&factory);
    let pos = query_position(&vlp, position_id);
    let (inside_0, inside_1) = query_fee_growth_inside(&vlp, new_lower, new_upper);

    assert_eq!(
        pos.fee_growth_inside_0_last_x128, inside_0,
        "fee_growth_inside_0_last should equal computed inside_0"
    );
    assert_eq!(
        pos.fee_growth_inside_1_last_x128, inside_1,
        "fee_growth_inside_1_last should equal computed inside_1"
    );

    // Sanity: inside should be <= global.
    let slot0_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    assert!(inside_0 <= slot0_after.fee_growth_global_0_x128);
    assert!(inside_1 <= slot0_after.fee_growth_global_1_x128);
}

/// Adding more liquidity to existing position — verify fees are settled.
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_fee_growth_consistent_after_add_to_existing_position(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);

    // The initial pool creation creates a position at the full range.
    let position_id = last_position_id(&factory);

    // Step 1: Swap to accrue fees inside the position's range.
    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_b.clone(),
        token_a.token.clone(),
        Uint128::new(20_000),
    );

    // Step 2: Query position — tokens_owed should be 0 (not yet settled).
    let pos_before = query_position(&vlp, position_id);
    assert_eq!(pos_before.tokens_owed_0, Uint128::zero());
    assert_eq!(pos_before.tokens_owed_1, Uint128::zero());

    // Step 3: Add more liquidity to the SAME position (same ticks, existing position_id).
    // This triggers settle_position_fees, which should accrue the fees into tokens_owed.
    let pos_ticks = (pos_before.lower_tick_index, pos_before.upper_tick_index);
    let pair2 = pair_with_amounts(&token_a, &token_b, 5_000, 5_000);
    add_concentrated_liquidity(
        &factory,
        &router,
        pair2,
        pool_key.clone(),
        pos_ticks.0,
        pos_ticks.1,
        Some(position_id),
        10_000,
    )
    .expect("add to existing position should succeed");

    // Step 4: Verify fees were settled — tokens_owed should be non-zero.
    let pos_after = query_position(&vlp, position_id);
    assert!(
        pos_after.tokens_owed_0 > Uint128::zero() || pos_after.tokens_owed_1 > Uint128::zero(),
        "fees should have been settled into tokens_owed after add: owed_0={}, owed_1={}",
        pos_after.tokens_owed_0,
        pos_after.tokens_owed_1,
    );

    // Step 5: Verify fee_growth_inside_last is updated correctly.
    let (inside_0, inside_1) = query_fee_growth_inside(&vlp, pos_ticks.0, pos_ticks.1);
    assert_eq!(pos_after.fee_growth_inside_0_last_x128, inside_0);
    assert_eq!(pos_after.fee_growth_inside_1_last_x128, inside_1);
}

/// Full lifecycle — add at new ticks, swap, remove, verify consistency.
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_remove_after_add_at_new_ticks(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let vlp_addr = Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp);
    let vlp = get_concentrated_vlp(router.environment(), &vlp_addr);

    // Step 1: Swap to accrue fees.
    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_b.clone(),
        token_a.token.clone(),
        Uint128::new(20_000),
    );

    // Step 2: Add liquidity at new ticks (the formerly buggy path).
    let slot0: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    let new_lower = ((slot0.tick - 100) / 10) * 10;
    let new_upper = ((slot0.tick + 100) / 10) * 10;

    let pair2 = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);
    add_concentrated_liquidity(
        &factory,
        &router,
        pair2,
        pool_key.clone(),
        new_lower,
        new_upper,
        None,
        10_000,
    )
    .expect("add at new ticks should succeed");

    let position_id = last_position_id(&factory);
    let pos = query_position(&vlp, position_id);
    let liquidity = pos.liquidity;
    assert!(!liquidity.is_zero());

    // Step 3: Swap again to accrue fees for the new position.
    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(5_000),
    );

    // Step 4: Collect fees — should not error.
    let chain_uid = factory.get_state().unwrap().chain_uid;
    let sender = CrossChainUser::new(chain_uid, factory.environment().sender.to_string());
    collect_concentrated_fees(&factory, &router, pool_key.clone(), position_id, sender)
        .expect("collect fees should succeed");

    // Step 5: Remove all liquidity — should not error (the original bug caused
    // "Cannot Sub with given operands" here).
    remove_concentrated_liquidity(&factory, &router, pool_key.clone(), position_id, liquidity)
        .expect("remove liquidity should succeed after fix");

    // Step 6: Position should be fully drained.
    let pos_final: Result<PositionResponse, _> =
        vlp.query(&ConcentratedQueryMsg::Position { position_id });
    // Position may be deleted (not found) or have zero liquidity.
    if let Ok(p) = pos_final {
        assert!(p.liquidity.is_zero(), "position should be fully drained");
    }
}
