#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::base::VlpSimulateSwapMsg;
use euclid::msgs::vlp::concentrated::msg::{QueryMsg as ConcentratedQueryMsg, Slot0Response};
use rstest::rstest;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{add_concentrated_liquidity, create_concentrated_pool};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_exact_single_range_quote_execution_parity(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );
    let simulation: euclid::msgs::vlp::base::GetSwapQueryResponse = vlp
        .query(&ConcentratedQueryMsg::SimulateSwap(VlpSimulateSwapMsg {
            asset: token_a.token.clone(),
            asset_amount: Uint128::new(2_500),
            swaps: vec![],
        }))
        .unwrap();

    let amount_out = execute_concentrated_swap(
        &factory,
        &router,
        pool_key,
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(2_500),
    );
    assert_eq!(amount_out, simulation.amount_out);
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_tick_crossing_updates_slot0_liquidity(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 80_000, 80_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 10_000, 10_000),
        pool_key.clone(),
        -20,
        20,
        None,
        100,
    )
    .unwrap();

    let vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );
    let before: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();

    let amount_out = execute_concentrated_swap(
        &factory,
        &router,
        pool_key,
        token_a,
        token_b.token,
        Uint128::new(15_000),
    );
    assert!(amount_out > Uint128::zero());

    let after: Slot0Response = vlp.query(&ConcentratedQueryMsg::Slot0 {}).unwrap();
    assert_ne!(before.tick, after.tick, "active tick should move after a large swap");
    assert!(
        after.liquidity <= before.liquidity,
        "crossing out of a narrow range should not increase active liquidity"
    );
}
