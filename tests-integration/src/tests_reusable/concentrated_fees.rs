#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::QueryMsg as ConcentratedQueryMsg;
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
fn test_fee_accrual_and_collect(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );
    let before: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse =
        vlp.query(&ConcentratedQueryMsg::TotalFeesCollected {}).unwrap();
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
    assert!(amount_out > Uint128::zero(), "swap must return non-zero output");

    let after: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse =
        vlp.query(&ConcentratedQueryMsg::TotalFeesCollected {}).unwrap();
    let after_fee = after
        .total_fees
        .lp_fees
        .get_fee(token_a.token.to_string().as_str());
    assert!(after_fee > before_fee, "fee accrual should increase after swap");
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

    let first: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse =
        vlp.query(&ConcentratedQueryMsg::TotalFeesCollected {}).unwrap();
    let second: euclid::msgs::vlp::concentrated::msg::TotalFeesResponse =
        vlp.query(&ConcentratedQueryMsg::TotalFeesCollected {}).unwrap();
    assert_eq!(first, second, "collect state should be idempotent without new swaps");
}
