#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Uint128, Uint256, Uint64};
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::{
    cross_chain_user::CrossChainUser,
    msgs::{
        factory::msg::QueryMsgFns as FactoryQueryMsgFns,
        router::execute::ExecuteMsgFns as RouterExecuteMsgFns,
        router::execute::ManageRouterState,
        router::query::QueryMsgFns as RouterQueryMsgFns,
        router::query::{QueryMsg as RouterQueryMsg, QuerySimulateSwap, SimulateSwapResponse},
        virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns,
        vlp::base::{PoolConfig, PoolKey, PoolType},
    },
    swap::NextSwapPair,
    token::{Pair, PairWithDenomAndAmount, Token, TokenType, TokenWithDenom},
    voucher::BalanceKey,
};
use factory::FactoryContract;
use router::RouterContract;
use rstest::rstest;

use crate::helpers::{
    chains::{get_virtual_balance, setup_router},
    factory::{create_concentrated_pool, create_pool},
};
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
};
use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::register_denom;
use crate::tests_reusable::factory_swap::swap_request;

fn native_token(name: &str) -> TokenWithDenom {
    TokenWithDenom {
        token: Token::create(name.to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: name.to_string(),
            decimals: Some(6),
        },
    }
}

fn setup_mixed_env(
    mode: FactorySetupMode,
    factory_chain_id: &str,
) -> (
    MockInterchainEnv,
    FactoryContract<MockBase>,
    RouterContract<MockBase>,
    TokenWithDenom,
    TokenWithDenom,
    TokenWithDenom,
    TokenWithDenom,
) {
    let sender = "sender_for_all_chains";
    let mut chains = vec![(ROUTER_CHAIN_ID, sender)];
    if factory_chain_id != ROUTER_CHAIN_ID {
        chains.push((factory_chain_id, sender));
    }

    let interchain = MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
    let factory = setup_factory_with_mode(&interchain, factory_chain_id, &router, mode).unwrap();

    let token_a = native_token("mix.token.a");
    let token_b = native_token("mix.token.b");
    let token_c = native_token("mix.token.c");
    let token_d = native_token("mix.token.d");

    register_denom(&factory, &router, token_a.clone()).unwrap();
    register_denom(&factory, &router, token_b.clone()).unwrap();
    register_denom(&factory, &router, token_c.clone()).unwrap();
    register_denom(&factory, &router, token_d.clone()).unwrap();

    (
        interchain, factory, router, token_a, token_b, token_c, token_d,
    )
}

#[allow(clippy::type_complexity)]
fn setup_mixed_route_pools(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token_a: &TokenWithDenom,
    token_b: &TokenWithDenom,
    token_c: &TokenWithDenom,
    token_d: &TokenWithDenom,
) -> PoolKey {
    create_pool(
        factory,
        router,
        PairWithDenomAndAmount {
            token_1: token_a.with_amount(Uint256::from(50_000u128)),
            token_2: token_b.with_amount(Uint256::from(50_000u128)),
        },
        100,
        PoolConfig::Stable {
            amp_factor: Some(Uint64::new(100)),
        },
    )
    .unwrap();

    let cl_pool_key = create_concentrated_pool(
        factory,
        router,
        PairWithDenomAndAmount {
            token_1: token_b.with_amount(Uint256::from(50_000u128)),
            token_2: token_c.with_amount(Uint256::from(50_000u128)),
        },
        500,
        10,
        100,
    )
    .unwrap();

    create_pool(
        factory,
        router,
        PairWithDenomAndAmount {
            token_1: token_c.with_amount(Uint256::from(50_000u128)),
            token_2: token_d.with_amount(Uint256::from(50_000u128)),
        },
        100,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    cl_pool_key
}

fn mixed_route(
    token_a: &TokenWithDenom,
    token_b: &TokenWithDenom,
    token_c: &TokenWithDenom,
    token_d: &TokenWithDenom,
    middle_pool_key: Option<PoolKey>,
) -> Vec<NextSwapPair> {
    vec![
        NextSwapPair {
            token_in: token_a.token.clone(),
            token_out: token_b.token.clone(),
            pool_key: None,
            test_fail: None,
        },
        NextSwapPair {
            token_in: token_b.token.clone(),
            token_out: token_c.token.clone(),
            pool_key: middle_pool_key,
            test_fail: None,
        },
        NextSwapPair {
            token_in: token_c.token.clone(),
            token_out: token_d.token.clone(),
            pool_key: None,
            test_fail: None,
        },
    ]
}

fn get_sender(factory: &FactoryContract<MockBase>) -> CrossChainUser {
    CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    )
}

fn get_voucher_balance(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: &Token,
) -> Uint256 {
    let sender = get_sender(factory);
    let virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );
    virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender,
            token_id: token.to_string(),
        })
        .unwrap()
        .amount
}

fn execute_swap_and_get_output(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token_in: TokenWithDenom,
    token_out: Token,
    amount_in: Uint256,
    swaps: Vec<NextSwapPair>,
) -> Result<Uint256, CwOrchError> {
    let before = get_voucher_balance(factory, router, &token_out);
    swap_request(
        factory,
        router,
        token_in,
        token_out.clone(),
        amount_in,
        Uint256::from(1u128),
        swaps,
        vec![],
        None,
    )?;
    let after = get_voucher_balance(factory, router, &token_out);
    Ok(after.checked_sub(before).unwrap_or(Uint256::zero()))
}

fn simulate_mixed_route(
    router: &RouterContract<MockBase>,
    asset_in: Token,
    asset_out: Token,
    amount_in: Uint256,
    swaps: Vec<NextSwapPair>,
) -> Uint256 {
    simulate_mixed_route_as(router, asset_in, asset_out, amount_in, swaps, None)
}

fn simulate_mixed_route_as(
    router: &RouterContract<MockBase>,
    asset_in: Token,
    asset_out: Token,
    amount_in: Uint256,
    swaps: Vec<NextSwapPair>,
    sender: Option<CrossChainUser>,
) -> Uint256 {
    let simulation: SimulateSwapResponse = router
        .query(&RouterQueryMsg::SimulateSwap(QuerySimulateSwap {
            asset_in,
            amount_in,
            asset_out,
            min_amount_out: Uint256::from(1u128),
            swaps,
            sender,
        }))
        .unwrap();
    simulation.amount_out
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_multihop_stable_clp_cp_executes(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b, token_c, token_d) =
        setup_mixed_env(mode, factory_chain_id);
    let middle_pool_key =
        setup_mixed_route_pools(&factory, &router, &token_a, &token_b, &token_c, &token_d);

    let output = execute_swap_and_get_output(
        &factory,
        &router,
        token_a.clone(),
        token_d.token.clone(),
        Uint256::from(1_000u128),
        mixed_route(
            &token_a,
            &token_b,
            &token_c,
            &token_d,
            Some(middle_pool_key),
        ),
    )
    .unwrap();
    assert!(output > Uint256::zero());
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_mixed_route_simulation_matches_execution(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b, token_c, token_d) =
        setup_mixed_env(mode, factory_chain_id);
    let middle_pool_key =
        setup_mixed_route_pools(&factory, &router, &token_a, &token_b, &token_c, &token_d);

    let amount_in = Uint256::from(1_000u128);
    let route = mixed_route(
        &token_a,
        &token_b,
        &token_c,
        &token_d,
        Some(middle_pool_key),
    );

    // VLPs now store reserves in voucher units (24 decimals). The router's
    // SimulateSwap query does NOT normalize the input, so we must pass
    // voucher-unit amounts to get results comparable to the execution path
    // (which normalizes via the factory/router deposit flow).
    let decimals_a = token_a.token_type.get_decimals().unwrap();
    let voucher_amount_in =
        euclid::normalize::normalize_token_to_voucher(amount_in, decimals_a).unwrap();
    let simulated = simulate_mixed_route(
        &router,
        token_a.token.clone(),
        token_d.token.clone(),
        voucher_amount_in,
        route.clone(),
    );

    let executed = execute_swap_and_get_output(
        &factory,
        &router,
        token_a.clone(),
        token_d.token.clone(),
        amount_in,
        route,
    )
    .unwrap();
    assert_eq!(executed, simulated);
}

/// SC-23 Issue 8 Slice 2 — the wallet's Euclid-fee override must propagate
/// *through* the CLP hop to the downstream legs, in both simulation and
/// execution.
///
/// The route is stable(a->b) -> CLP(b->c) -> cp(c->d). CLP pools are created
/// with a protocol cut of 0, so the override is a no-op on the CLP leg itself —
/// but the cp leg sits *after* the CLP and carries a non-zero Euclid fee, so it
/// only receives the discount if the CLP forwards the override onward. A broken
/// forwarder would leave the cp leg at the full fee; the assertions below would
/// then fail.
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_override_propagates_through_clp_hop(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b, token_c, token_d) =
        setup_mixed_env(mode, factory_chain_id);
    let middle_pool_key =
        setup_mixed_route_pools(&factory, &router, &token_a, &token_b, &token_c, &token_d);

    let wallet = get_sender(&factory);
    let amount_in = Uint256::from(1_000u128);
    let decimals_a = token_a.token_type.get_decimals().unwrap();
    let voucher_amount_in =
        euclid::normalize::normalize_token_to_voucher(amount_in, decimals_a).unwrap();
    let route = mixed_route(
        &token_a,
        &token_b,
        &token_c,
        &token_d,
        Some(middle_pool_key),
    );

    // Full exemption for this wallet (fee-admin gated; the router's deployer is
    // the fee admin in this harness).
    router
        .manage_router_state(ManageRouterState::SetEuclidFeeOverride {
            user: wallet.clone(),
            euclid_fee_bps: Some(0),
        })
        .unwrap();

    // Same route, quoted with the wallet's override vs. without any sender.
    let sim_with = simulate_mixed_route_as(
        &router,
        token_a.token.clone(),
        token_d.token.clone(),
        voucher_amount_in,
        route.clone(),
        Some(wallet.clone()),
    );
    let sim_without = simulate_mixed_route_as(
        &router,
        token_a.token.clone(),
        token_d.token.clone(),
        voucher_amount_in,
        route.clone(),
        None,
    );
    assert!(
        sim_with > sim_without,
        "override must improve the multi-hop quote (incl. the post-CLP cp leg): \
         with={sim_with}, without={sim_without}"
    );

    // Execution resolves the same wallet's override and forwards it identically;
    // the executed output must match the override-applied quote exactly.
    let executed = execute_swap_and_get_output(
        &factory,
        &router,
        token_a.clone(),
        token_d.token.clone(),
        amount_in,
        route,
    )
    .unwrap();
    assert_eq!(
        executed, sim_with,
        "executed multi-hop output must match the override-applied simulation"
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_mixed_route_fee_tier_selection_is_explicit(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b, token_c, token_d) =
        setup_mixed_env(mode, factory_chain_id);
    let pool_500 =
        setup_mixed_route_pools(&factory, &router, &token_a, &token_b, &token_c, &token_d);
    let pool_3000 = create_concentrated_pool(
        &factory,
        &router,
        PairWithDenomAndAmount {
            token_1: token_b.with_amount(Uint256::from(50_000u128)),
            token_2: token_c.with_amount(Uint256::from(50_000u128)),
        },
        3_000,
        60,
        100,
    )
    .unwrap();

    let amount_in = Uint256::from(1_000u128);
    let sim_500 = simulate_mixed_route(
        &router,
        token_a.token.clone(),
        token_d.token.clone(),
        amount_in,
        mixed_route(&token_a, &token_b, &token_c, &token_d, Some(pool_500)),
    );
    let sim_3000 = simulate_mixed_route(
        &router,
        token_a.token.clone(),
        token_d.token.clone(),
        amount_in,
        mixed_route(&token_a, &token_b, &token_c, &token_d, Some(pool_3000)),
    );

    assert_ne!(sim_500, sim_3000);
    assert!(
        sim_500 > sim_3000,
        "lower fee tier route should output more for equal reserves",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_missing_pool_key_does_not_use_clp(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b, token_c, token_d) =
        setup_mixed_env(mode, factory_chain_id);
    setup_mixed_route_pools(&factory, &router, &token_a, &token_b, &token_c, &token_d);

    let before = get_voucher_balance(&factory, &router, &token_d.token);
    let result = swap_request(
        &factory,
        &router,
        token_a.clone(),
        token_d.token.clone(),
        Uint256::from(1_000u128),
        Uint256::from(1u128),
        mixed_route(&token_a, &token_b, &token_c, &token_d, None),
        vec![],
        None,
    );
    let after = get_voucher_balance(&factory, &router, &token_d.token);

    match mode {
        FactorySetupMode::Native => assert!(result.is_err()),
        FactorySetupMode::Ibc | FactorySetupMode::Evm => assert!(result.is_ok()),
    }
    assert_eq!(after, before, "route without pool_key must not use CLP");
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_invalid_pool_key_pair_mismatch_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b, token_c, token_d) =
        setup_mixed_env(mode, factory_chain_id);
    let middle_pool_key =
        setup_mixed_route_pools(&factory, &router, &token_a, &token_b, &token_c, &token_d);

    let bad_route = vec![
        NextSwapPair {
            token_in: token_a.token.clone(),
            token_out: token_b.token.clone(),
            pool_key: Some(middle_pool_key),
            test_fail: None,
        },
        NextSwapPair {
            token_in: token_b.token.clone(),
            token_out: token_c.token.clone(),
            pool_key: None,
            test_fail: None,
        },
        NextSwapPair {
            token_in: token_c.token.clone(),
            token_out: token_d.token.clone(),
            pool_key: None,
            test_fail: None,
        },
    ];

    let err = swap_request(
        &factory,
        &router,
        token_a.clone(),
        token_d.token.clone(),
        Uint256::from(1_000u128),
        Uint256::from(1u128),
        bad_route,
        vec![],
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(!err.is_empty(), "expected pool-key pair mismatch failure",);
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_invalid_pool_key_type_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b, token_c, token_d) =
        setup_mixed_env(mode, factory_chain_id);
    setup_mixed_route_pools(&factory, &router, &token_a, &token_b, &token_c, &token_d);

    let invalid_pool_key = PoolKey {
        pair: Pair::new(token_b.token.clone(), token_c.token.clone()).unwrap(),
        pool_type: PoolType::ConstantProduct {},
    };

    let route = mixed_route(
        &token_a,
        &token_b,
        &token_c,
        &token_d,
        Some(invalid_pool_key),
    );
    let err = swap_request(
        &factory,
        &router,
        token_a.clone(),
        token_d.token.clone(),
        Uint256::from(1_000u128),
        Uint256::from(1u128),
        route,
        vec![],
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(!err.is_empty(), "expected invalid pool-key type failure",);
}
