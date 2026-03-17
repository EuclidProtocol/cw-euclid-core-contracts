#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{LegacyLiquidityMode, QueryMsg as ConcentratedQueryMsg};
use rstest::rstest;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, collect_concentrated_fees, create_concentrated_pool,
    list_position_ids, migrate_concentrated_pool, query_concentrated_pool_migration_status,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

fn first_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    assert!(!ids.is_empty(), "expected at least one position");
    Uint128::new(ids[0].parse::<u128>().unwrap())
}

fn run_migrate_rebuild_swap_collect(mode: FactorySetupMode, factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pool_key = create_concentrated_pool(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 40_000, 40_000),
        500,
        10,
        100,
    )
    .unwrap();

    let before = execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(2_000),
    );
    assert!(before > Uint128::zero());

    migrate_concentrated_pool(
        &factory,
        &router,
        pool_key.clone(),
        LegacyLiquidityMode::AlreadyV3Liquidity,
        None,
        Some(true),
    )
    .unwrap();

    let after = execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(2_000),
    );
    assert!(
        after > Uint128::zero(),
        "post-migration swap should succeed"
    );

    let sender = CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    );
    collect_concentrated_fees(
        &factory,
        &router,
        pool_key.clone(),
        first_position_id(&factory),
        sender,
    )
    .unwrap();

    let status = query_concentrated_pool_migration_status(&factory, &router, pool_key).unwrap();
    assert_eq!(status.revision, 2);
}

#[test]
fn test_migrate_rebuild_then_swap_collect_native() {
    run_migrate_rebuild_swap_collect(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL);
}

#[test]
fn test_migrate_rebuild_then_swap_collect_ibc() {
    run_migrate_rebuild_swap_collect(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC);
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_migration_status_and_invariants_exposed(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    let pool_key = create_concentrated_pool(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 50_000, 50_000),
        500,
        10,
        100,
    )
    .unwrap();

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 10_000, 10_000),
        pool_key.clone(),
        -20,
        20,
        None,
        10_000,
    )
    .unwrap();

    migrate_concentrated_pool(
        &factory,
        &router,
        pool_key.clone(),
        LegacyLiquidityMode::AlreadyV3Liquidity,
        None,
        Some(true),
    )
    .unwrap();

    let status =
        query_concentrated_pool_migration_status(&factory, &router, pool_key.clone()).unwrap();
    assert_eq!(status.revision, 2);
    assert!(status.positions_migrated >= 1);

    let vlp_addr = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_addr));

    let slot0: euclid::msgs::vlp::concentrated::msg::Slot0Response =
        vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    let ticks: euclid::msgs::vlp::concentrated::msg::TicksResponse = vlp
        .query(&ConcentratedQueryMsg::Ticks {
            start_after: None,
            limit: Some(200),
        })
        .unwrap();
    assert!(!ticks.ticks.is_empty());
    assert!(ticks.ticks.iter().all(|tick| tick.initialized));

    let mut sum_liquidity = Uint128::zero();
    for id in list_position_ids(&factory).unwrap() {
        let position: euclid::msgs::vlp::concentrated::msg::PositionResponse = vlp
            .query(&ConcentratedQueryMsg::Position {
                position_id: Uint128::new(id.parse::<u128>().unwrap()),
            })
            .unwrap();
        sum_liquidity = sum_liquidity.checked_add(position.liquidity).unwrap();
    }

    assert_eq!(status.total_liquidity, sum_liquidity);
    assert_eq!(status.active_liquidity, slot0.liquidity);
}
