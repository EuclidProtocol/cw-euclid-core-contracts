#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint256};
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{ObserveResponse, QueryMsg as ConcentratedQueryMsg};
use rstest::rstest;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::create_concentrated_pool;
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_observe_returns_valid_cumulatives(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint256::from(2_000u128),
    );
    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_b.clone(),
        token_a.token.clone(),
        Uint256::from(1_000u128),
    );

    let vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key).unwrap().vlp),
    );
    let observe: ObserveResponse = vlp
        .query(&ConcentratedQueryMsg::Observe {
            seconds_agos: vec![0, 1],
        })
        .unwrap();

    assert_eq!(observe.tick_cumulatives.len(), 2);
    assert_eq!(observe.seconds_per_liquidity_cumulative_x128s.len(), 2);
    assert!(
        observe.tick_cumulatives[0] >= observe.tick_cumulatives[1],
        "more recent cumulative should be >= older cumulative"
    );
}
