#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::{
    ExecuteApprove, ExecuteMsg as VirtualBalanceExecuteMsg,
    QueryMsgFns as VirtualBalanceQueryMsgFns,
};
use euclid::msgs::vlp::base::{VlpSimulateSwapMsg, VlpSwapMsg};
use euclid::msgs::vlp::concentrated::msg::QueryMsg as ConcentratedQueryMsg;
use euclid::normalize::{normalize_token_to_voucher, normalize_voucher_to_token};
use euclid::voucher::BalanceKey;
use rstest::rstest;

use crate::helpers::chains::{get_concentrated_vlp, get_virtual_balance};
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, deposit_token,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

pub fn execute_concentrated_swap(
    factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    pool_key: euclid::msgs::vlp::base::PoolKey,
    asset_in: euclid::token::TokenWithDenom,
    asset_out: euclid::token::Token,
    amount_in: Uint256,
) -> Uint256 {
    let voucher_amount = if asset_in.token_type.is_voucher() {
        amount_in
    } else {
        let decimals = asset_in
            .token_type
            .get_decimals()
            .expect("token type should have decimals");
        euclid::normalize::normalize_token_to_voucher(amount_in, decimals)
            .expect("normalization should succeed")
    };

    let chain_uid = factory.get_state().unwrap().chain_uid;
    let sender = CrossChainUser::new(chain_uid, factory.environment().sender.to_string());
    deposit_token(factory, router, asset_in.clone(), amount_in, vec![]).unwrap();

    let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let mut vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address.clone()));

    let mut virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );
    virtual_balance.set_sender(&router.address().unwrap());
    virtual_balance
        .execute(
            &VirtualBalanceExecuteMsg::Approve(ExecuteApprove {
                amount: voucher_amount,
                token_id: asset_in.token.to_string(),
                spender: CrossChainUser::new(
                    euclid::chain::ChainUid::vsl_chain_uid().unwrap(),
                    vlp_address.clone(),
                ),
                owner: sender.clone(),
            }),
            &[],
        )
        .unwrap();

    let before_out = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: asset_out.to_string(),
        })
        .unwrap()
        .amount;

    vlp.set_sender(&router.address().unwrap());
    vlp.execute(
        &euclid::msgs::vlp::concentrated::msg::ExecuteMsg::Swap(VlpSwapMsg {
            sender: sender.clone(),
            tx_id: "concentrated_swap".to_string(),
            asset_in: asset_in.token,
            amount_in: voucher_amount,
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: None,
            euclid_fee_override: None,
        }),
        &[],
    )
    .unwrap();

    let after_out = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender,
            token_id: asset_out.to_string(),
        })
        .unwrap()
        .amount;
    after_out - before_out
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_swap_single_range(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let raw_amount = Uint256::from(1_000u128);
    let decimals = token_a
        .token_type
        .get_decimals()
        .expect("token should have decimals");
    let voucher_amount =
        normalize_token_to_voucher(raw_amount, decimals).expect("normalization should succeed");

    let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let simulation: euclid::msgs::vlp::base::GetSwapQueryResponse = vlp
        .query(&ConcentratedQueryMsg::SimulateSwap(VlpSimulateSwapMsg {
            asset: token_a.token.clone(),
            asset_amount: voucher_amount,
            swaps: vec![],
            euclid_fee_override: None,
        }))
        .unwrap();

    let amount_out = execute_concentrated_swap(
        &factory,
        &router,
        pool_key,
        token_a.clone(),
        token_b.token.clone(),
        raw_amount,
    );
    assert_eq!(amount_out, simulation.amount_out);
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_swap_crosses_ticks(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 20_000, 20_000),
        pool_key.clone(),
        -120,
        120,
        None,
        100,
    )
    .unwrap();

    let vlp_address = router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let chain_uid = factory.get_state().unwrap().chain_uid;
    let before: euclid::msgs::vlp::concentrated::msg::ConcentratedPoolResponse = vlp
        .query(&ConcentratedQueryMsg::Pool {
            chain_uid: chain_uid.clone(),
            pool_key: pool_key.clone(),
        })
        .unwrap();

    let amount_out = execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint256::from(8_000u128),
    );
    assert!(amount_out > Uint256::zero());

    let after: euclid::msgs::vlp::concentrated::msg::ConcentratedPoolResponse = vlp
        .query(&ConcentratedQueryMsg::Pool {
            chain_uid,
            pool_key,
        })
        .unwrap();

    assert!(
        after.reserve_1 > before.reserve_1,
        "input reserve should increase"
    );
    assert!(
        after.reserve_2 < before.reserve_2,
        "output reserve should decrease"
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_swap_explicit_fee_tier_routing(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
    let pool_500 = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();
    let pool_3000 =
        create_concentrated_pool(&factory, &router, pair.clone(), 3_000, 60, 100).unwrap();

    let vlp_500 = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_500).unwrap().vlp),
    );
    let vlp_3000 = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_3000).unwrap().vlp),
    );

    let decimals = token_a
        .token_type
        .get_decimals()
        .expect("token should have decimals");
    let voucher_amount = normalize_token_to_voucher(Uint256::from(1_000u128), decimals)
        .expect("normalization should succeed");

    let sim_500: euclid::msgs::vlp::base::GetSwapQueryResponse = vlp_500
        .query(&ConcentratedQueryMsg::SimulateSwap(VlpSimulateSwapMsg {
            asset: token_a.token.clone(),
            asset_amount: voucher_amount,
            swaps: vec![],
            euclid_fee_override: None,
        }))
        .unwrap();
    let sim_3000: euclid::msgs::vlp::base::GetSwapQueryResponse = vlp_3000
        .query(&ConcentratedQueryMsg::SimulateSwap(VlpSimulateSwapMsg {
            asset: token_a.token.clone(),
            asset_amount: voucher_amount,
            swaps: vec![],
            euclid_fee_override: None,
        }))
        .unwrap();

    assert_ne!(sim_500.amount_out, sim_3000.amount_out);
    assert!(
        sim_500.amount_out > sim_3000.amount_out,
        "lower fee tier should return more output for same reserves",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_larger_swap_has_worse_effective_price(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let vlp_address = router.get_vlp_by_pool_key(pool_key).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    let decimals = token_a
        .token_type
        .get_decimals()
        .expect("token should have decimals");
    let small_in = Uint128::new(1_000);
    let large_in = Uint128::new(5_000);
    let small_in_voucher = normalize_token_to_voucher(Uint256::from(small_in), decimals).unwrap();
    let large_in_voucher = normalize_token_to_voucher(Uint256::from(large_in), decimals).unwrap();

    let small: euclid::msgs::vlp::base::GetSwapQueryResponse = vlp
        .query(&ConcentratedQueryMsg::SimulateSwap(VlpSimulateSwapMsg {
            asset: token_a.token.clone(),
            asset_amount: small_in_voucher,
            swaps: vec![],
            euclid_fee_override: None,
        }))
        .unwrap();
    let large: euclid::msgs::vlp::base::GetSwapQueryResponse = vlp
        .query(&ConcentratedQueryMsg::SimulateSwap(VlpSimulateSwapMsg {
            asset: token_a.token.clone(),
            asset_amount: large_in_voucher,
            swaps: vec![],
            euclid_fee_override: None,
        }))
        .unwrap();

    assert!(small.amount_out > Uint256::zero());
    assert!(large.amount_out > Uint256::zero());

    let small_effective_numerator = small.amount_out * large_in_voucher;
    let large_effective_numerator = large.amount_out * small_in_voucher;
    assert!(
        small_effective_numerator > large_effective_numerator,
        "larger trades should receive a worse effective price due to curve impact",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_round_trip_swap_loses_value(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let raw_amount_in = Uint256::from(2_000u128);
    let decimals_a = token_a
        .token_type
        .get_decimals()
        .expect("token_a should have decimals");
    let decimals_b = token_b
        .token_type
        .get_decimals()
        .expect("token_b should have decimals");
    let voucher_amount_in = normalize_token_to_voucher(raw_amount_in, decimals_a)
        .expect("normalization should succeed");

    let amount_out = execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        raw_amount_in,
    );
    assert!(amount_out > Uint256::zero());

    // amount_out is in voucher units. execute_concentrated_swap normalizes
    // raw->voucher internally, so denormalize back to raw for the second swap.
    let raw_amount_out =
        normalize_voucher_to_token(amount_out, decimals_b).expect("denormalization should succeed");

    let amount_back = execute_concentrated_swap(
        &factory,
        &router,
        pool_key,
        token_b.clone(),
        token_a.token.clone(),
        raw_amount_out,
    );

    assert!(
        amount_back < voucher_amount_in,
        "round-trip should lose value from swap fees/price impact",
    );
}
