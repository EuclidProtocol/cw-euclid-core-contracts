#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::msgs::vlp::concentrated::msg::QueryMsg as ConcentratedQueryMsg;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use rstest::rstest;

use crate::helpers::chains::get_concentrated_vlp;
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, get_position_token, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
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
    let vlp_address = router
        .get_vlp_by_pool_key(pool_key.clone())
        .unwrap()
        .vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let chain_uid = factory.get_state().unwrap().chain_uid;
    let pool: euclid::msgs::vlp::concentrated::msg::ConcentratedPoolResponse = vlp
        .query(&ConcentratedQueryMsg::Pool { chain_uid, pool_key })
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
        .query::<position_token::msg::TokensResponse>(&position_token::msg::QueryMsg::AllTokens {})
        .unwrap()
        .tokens;
    assert_eq!(tokens.len(), 1, "initial add should mint exactly one NFT");

    let owner = position_token
        .query::<position_token::msg::OwnerOfResponse>(&position_token::msg::QueryMsg::OwnerOf {
            token_id: tokens[0].clone(),
        })
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
    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
        pool_key,
        -120,
        120,
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
fn test_full_remove_burns_position(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
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
