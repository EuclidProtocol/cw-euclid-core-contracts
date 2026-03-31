#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    PositionResponse, QueryMsg as ConcentratedQueryMsg, Slot0Response,
};
use rstest::rstest;
use std::collections::HashSet;

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

fn first_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    assert!(!ids.is_empty(), "expected at least one position");
    Uint128::new(ids[0].parse::<u128>().unwrap())
}

fn pool_lp_shares(
    factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    pool_key: euclid::msgs::vlp::base::PoolKey,
) -> Uint128 {
    let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let chain_uid = factory.get_state().unwrap().chain_uid;
    let pool: euclid::msgs::vlp::concentrated::msg::ConcentratedPoolResponse = vlp
        .query(&ConcentratedQueryMsg::Pool {
            chain_uid,
            pool_key,
        })
        .unwrap();
    pool.lp_shares
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_add_liquidity_mints_position_nft(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let _pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let position_token = get_position_token(&factory).unwrap();
    let tokens = position_token
        .query::<euclid::msgs::position_token::TokensResponse>(
            &euclid::msgs::position_token::QueryMsg::AllTokens { start_after: None, limit: None },
        )
        .unwrap()
        .tokens;
    assert_eq!(tokens.len(), 1, "initial add should mint exactly one NFT");

    let owner = position_token
        .query::<euclid::msgs::position_token::OwnerOfResponse>(
            &euclid::msgs::position_token::QueryMsg::OwnerOf {
                token_id: tokens[0].clone(),
            },
        )
        .unwrap()
        .owner;
    assert_eq!(owner, factory.environment().sender.to_string());
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_increase_liquidity_updates_same_position(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let initial_position_id = first_position_id(&factory);
    let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let initial_position: euclid::msgs::vlp::concentrated::msg::PositionResponse = vlp
        .query(&ConcentratedQueryMsg::Position {
            position_id: initial_position_id,
        })
        .unwrap();

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
        pool_key,
        initial_position.lower_tick_index,
        initial_position.upper_tick_index,
        Some(initial_position_id),
        100,
    )
    .unwrap();

    let ids = list_position_ids(&factory).unwrap();
    assert_eq!(ids.len(), 1, "increase should not mint a new position NFT");
    assert_eq!(ids[0], initial_position_id.to_string());
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_partial_decrease_keeps_position(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let position_id = first_position_id(&factory);
    let total_lp_shares = pool_lp_shares(&factory, &router, pool_key.clone());
    let partial = Uint128::new((total_lp_shares.u128() / 2).max(1));
    remove_concentrated_liquidity(&factory, &router, pool_key, position_id, partial).unwrap();

    let ids = list_position_ids(&factory).unwrap();
    assert!(
        ids.iter().any(|id| id == &position_id.to_string()),
        "position NFT should remain after partial remove",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_full_remove_burns_position(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let position_id = first_position_id(&factory);
    let total_lp_shares = pool_lp_shares(&factory, &router, pool_key.clone());
    remove_concentrated_liquidity(&factory, &router, pool_key, position_id, total_lp_shares)
        .unwrap();

    let ids = list_position_ids(&factory).unwrap();
    assert!(ids.is_empty(), "position NFT must be burned on full remove");
}

#[test]
fn test_only_owner_can_modify_or_collect() {
    let (_interchain, mut factory, router, token_a, token_b) =
        setup_concentrated_env(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();
    let position_id = first_position_id(&factory);

    let intruder = factory.environment().addr_make("intruder");
    factory.set_sender(&intruder);

    let add_err = add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 1_000, 1_000),
        pool_key.clone(),
        -120,
        120,
        Some(position_id),
        100,
    )
    .unwrap_err();
    assert!(
        !add_err.to_string().is_empty(),
        "expected unauthorized add liquidity to fail",
    );

    let remove_err =
        remove_concentrated_liquidity(&factory, &router, pool_key, position_id, Uint128::new(1))
            .unwrap_err();
    assert!(
        !remove_err.to_string().is_empty(),
        "expected unauthorized remove liquidity to fail",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_multiple_positions_different_ranges_are_independent(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let first_ids = list_position_ids(&factory).unwrap();
    assert_eq!(first_ids.len(), 1);
    let first_id = Uint128::new(first_ids[0].parse::<u128>().unwrap());

    let lp_before_second = pool_lp_shares(&factory, &router, pool_key.clone());
    add_concentrated_liquidity(
        &factory,
        &router,
        // Current tick is centered around 0 after pool creation.
        // This range is entirely below spot, so one side will be mostly unused.
        pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
        pool_key.clone(),
        -240,
        -120,
        None,
        10_000,
    )
    .unwrap();
    let lp_after_second = pool_lp_shares(&factory, &router, pool_key.clone());
    let second_position_liquidity = lp_after_second.checked_sub(lp_before_second).unwrap();
    assert!(
        second_position_liquidity > Uint128::zero(),
        "second range should mint non-zero liquidity",
    );

    let second_ids = list_position_ids(&factory).unwrap();
    assert_eq!(
        second_ids.len(),
        2,
        "adding with None position_id should mint a new NFT"
    );

    let first_id_set: HashSet<&str> = first_ids.iter().map(String::as_str).collect();
    let second_id = second_ids
        .iter()
        .find(|id| !first_id_set.contains(id.as_str()))
        .expect("must contain a newly minted position id");
    let second_id = Uint128::new(second_id.parse::<u128>().unwrap());

    remove_concentrated_liquidity(
        &factory,
        &router,
        pool_key,
        second_id,
        second_position_liquidity,
    )
    .unwrap();

    let final_ids = list_position_ids(&factory).unwrap();
    assert_eq!(
        final_ids.len(),
        1,
        "removing second position should not remove first"
    );
    assert_eq!(final_ids[0], first_id.to_string());
}

/// Regression: after many small swaps crossing a position boundary,
/// ACTIVE_LIQUIDITY must still match the sum of in-range positions.
/// Without the tick boundary rounding fix, the price can round back to a
/// just-crossed tick, desyncing the liquidity state.
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
        &factory,
        &router,
        pair2,
        pool_key.clone(),
        lower,
        upper,
        None,
        10_000,
    )
    .expect("add narrow position should succeed");

    // Many small swaps back and forth to repeatedly cross the position boundary
    for _ in 0..10 {
        let _ = execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_b.clone(),
            token_a.token.clone(),
            Uint128::new(300),
        );
        let _ = execute_concentrated_swap(
            &factory,
            &router,
            pool_key.clone(),
            token_a.clone(),
            token_b.token.clone(),
            Uint128::new(300),
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
    let price_tick =
        concentrated_vlp::math::tick_math::get_tick_at_sqrt_ratio(slot0.sqrt_price_x96).unwrap();
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
        slot0.liquidity.u128() == in_range_liquidity || slot0.liquidity.u128() == in_range_by_price,
        "ACTIVE_LIQUIDITY ({}) should match in-range positions \
         by tick ({}, tick={}) or by price ({}, price_tick={})",
        slot0.liquidity,
        in_range_liquidity,
        slot0.tick,
        in_range_by_price,
        price_tick,
    );
}
