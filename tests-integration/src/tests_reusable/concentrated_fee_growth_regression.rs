#![cfg(not(target_arch = "wasm32"))]

//! Regression test for fee_growth_inside corruption.
//!
//! Before the fix, `settle_position_fees` was called BEFORE `apply_liquidity_delta`
//! in `execute_add_concentrated_liquidity`. For a new position whose ticks don't
//! yet exist, settle computes `inside = global` (using default zero tick values),
//! then tick initialization sets `lower.outside = global`, making actual inside = 0
//! while `last = global`. This causes fee_growth_inside to go backward.
//!
//! The fix swaps the order to match Uniswap V3: Tick.update then Position.update.

use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response, TickResponse,
};
use rstest::rstest;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{add_concentrated_liquidity, create_concentrated_pool, list_position_ids};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

/// Computes fee_growth_inside for a position using the same logic as the contract.
fn compute_fee_growth_inside(
    slot0: &Slot0Response,
    lower_tick: &Option<TickResponse>,
    upper_tick: &Option<TickResponse>,
    lower_idx: i64,
    upper_idx: i64,
) -> (Uint256, Uint256) {
    let global_0 = slot0.fee_growth_global_0_x128;
    let global_1 = slot0.fee_growth_global_1_x128;

    let (lo_out_0, lo_out_1) = lower_tick
        .as_ref()
        .map(|t| (t.fee_growth_outside_0_x128, t.fee_growth_outside_1_x128))
        .unwrap_or((Uint256::zero(), Uint256::zero()));
    let (hi_out_0, hi_out_1) = upper_tick
        .as_ref()
        .map(|t| (t.fee_growth_outside_0_x128, t.fee_growth_outside_1_x128))
        .unwrap_or((Uint256::zero(), Uint256::zero()));

    let (below_0, below_1) = if slot0.tick >= lower_idx {
        (lo_out_0, lo_out_1)
    } else {
        (global_0 - lo_out_0, global_1 - lo_out_1)
    };
    let (above_0, above_1) = if slot0.tick < upper_idx {
        (hi_out_0, hi_out_1)
    } else {
        (global_0 - hi_out_0, global_1 - hi_out_1)
    };

    (global_0 - below_0 - above_0, global_1 - below_1 - above_1)
}

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

    // Step 1: Swap to accrue fees and move price (one_for_zero pushes tick up).
    execute_concentrated_swap(
        &factory, &router, pool_key.clone(),
        token_b.clone(), token_a.token.clone(), Uint128::new(30_000),
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
        &factory, &router, pair2, pool_key.clone(),
        new_lower, new_upper, None, 10_000,
    )
    .expect("add liquidity at new ticks should succeed");

    // Step 4: Query the new position and verify fee_growth_inside_last is consistent.
    let position_ids = list_position_ids(&factory).unwrap();
    let latest_id = position_ids.last().unwrap().parse::<u128>().unwrap();
    let pos: PositionResponse = vlp
        .query(&ConcentratedQueryMsg::Position {
            position_id: Uint128::new(latest_id),
        })
        .unwrap();

    // Compute what fee_growth_inside should be NOW (after tick initialization).
    let slot0_after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    let lower_tick: Option<TickResponse> = vlp
        .query(&ConcentratedQueryMsg::Tick { index: new_lower })
        .ok();
    let upper_tick: Option<TickResponse> = vlp
        .query(&ConcentratedQueryMsg::Tick { index: new_upper })
        .ok();

    let (inside_0, inside_1) = compute_fee_growth_inside(
        &slot0_after, &lower_tick, &upper_tick, new_lower, new_upper,
    );

    // The position's last should match the computed inside (both should be 0 for
    // a brand new position at fresh ticks where lower.outside = global).
    assert_eq!(
        pos.fee_growth_inside_0_last_x128, inside_0,
        "fee_growth_inside_0_last should equal computed inside_0: last={}, computed={}",
        pos.fee_growth_inside_0_last_x128, inside_0,
    );
    assert_eq!(
        pos.fee_growth_inside_1_last_x128, inside_1,
        "fee_growth_inside_1_last should equal computed inside_1: last={}, computed={}",
        pos.fee_growth_inside_1_last_x128, inside_1,
    );

    // Extra: inside should be <= global (sanity check).
    assert!(
        inside_0 <= slot0_after.fee_growth_global_0_x128,
        "inside_0 should not exceed global_0"
    );
    assert!(
        inside_1 <= slot0_after.fee_growth_global_1_x128,
        "inside_1 should not exceed global_1"
    );
}
