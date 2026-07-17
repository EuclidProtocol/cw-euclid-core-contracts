#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::QueryMsg as ConcentratedQueryMsg;
use euclid::voucher::BalanceKey;
use rstest::rstest;

use crate::helpers::chains::{get_concentrated_vlp, get_virtual_balance};
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, list_position_ids,
    remove_concentrated_liquidity,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

fn sender(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> CrossChainUser {
    CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    )
}

fn first_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    assert!(!ids.is_empty(), "expected at least one position");
    Uint128::new(ids[0].parse::<u128>().unwrap())
}

fn position(
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    pool_key: euclid::msgs::vlp::base::PoolKey,
    position_id: Uint128,
) -> euclid::msgs::vlp::concentrated::msg::PositionResponse {
    let vlp_address = router.get_vlp_by_pool_key(pool_key).unwrap().vlp;
    let vlp = get_concentrated_vlp(router.environment(), &Addr::unchecked(vlp_address));
    vlp.query(&ConcentratedQueryMsg::Position { position_id })
        .unwrap()
}

fn virtual_balance_for_token(
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    sender: CrossChainUser,
    token_id: String,
) -> Uint256 {
    let virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );
    virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender,
            token_id,
        })
        .unwrap()
        .amount
}

// Cross-VM coverage: none (CosmWasm-only)
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_in_range_imbalanced_add_refunds_excess(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let ccu = sender(&factory);
    let before_a = virtual_balance_for_token(&router, ccu.clone(), token_a.token.to_string());
    let before_b = virtual_balance_for_token(&router, ccu.clone(), token_b.token.to_string());

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 10_000, 1_000),
        pool_key,
        -10,
        10,
        None,
        9_000,
    )
    .unwrap();

    let after_a = virtual_balance_for_token(&router, ccu.clone(), token_a.token.to_string());
    let after_b = virtual_balance_for_token(&router, ccu, token_b.token.to_string());
    let refund_a = after_a.checked_sub(before_a).unwrap();
    let refund_b = after_b.checked_sub(before_b).unwrap();

    // Virtual balance stores amounts in voucher units (24 decimals).
    // For 6-decimal tokens the normalization factor is 10^18.
    let voucher_factor = Uint256::from(10u128.pow(18));
    assert!(
        refund_a >= Uint256::from(8_000u128) * voucher_factor,
        "expected large refund on excess in-range side",
    );
    assert!(
        refund_b <= Uint256::from(50u128) * voucher_factor,
        "limiting side should have no/low refund",
    );
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/one_sided.rs::clp_one_sided_below_range_add_and_remove
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_below_range_add_refunds_token1(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let ccu = sender(&factory);
    let before_a = virtual_balance_for_token(&router, ccu.clone(), token_a.token.to_string());
    let before_b = virtual_balance_for_token(&router, ccu.clone(), token_b.token.to_string());

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
        pool_key,
        100,
        200,
        None,
        10_000,
    )
    .unwrap();

    let after_a = virtual_balance_for_token(&router, ccu.clone(), token_a.token.to_string());
    let after_b = virtual_balance_for_token(&router, ccu, token_b.token.to_string());
    let refund_a = after_a.checked_sub(before_a).unwrap();
    let refund_b = after_b.checked_sub(before_b).unwrap();

    // Virtual balance stores amounts in voucher units (24 decimals).
    // For 6-decimal tokens the normalization factor is 10^18.
    let voucher_factor = Uint256::from(10u128.pow(18));
    assert_eq!(
        refund_b,
        Uint256::from(5_000u128) * voucher_factor,
        "token1 should be fully refunded when range is above current price",
    );
    assert!(
        refund_a <= Uint256::from(5u128) * voucher_factor,
        "token0 should be nearly fully consumed in below-range add",
    );
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/one_sided.rs::clp_one_sided_above_range_add_and_remove
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_above_range_add_refunds_token0(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let ccu = sender(&factory);
    let before_a = virtual_balance_for_token(&router, ccu.clone(), token_a.token.to_string());
    let before_b = virtual_balance_for_token(&router, ccu.clone(), token_b.token.to_string());

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 5_000, 5_000),
        pool_key,
        -200,
        -100,
        None,
        10_000,
    )
    .unwrap();

    let after_a = virtual_balance_for_token(&router, ccu.clone(), token_a.token.to_string());
    let after_b = virtual_balance_for_token(&router, ccu, token_b.token.to_string());
    let refund_a = after_a.checked_sub(before_a).unwrap();
    let refund_b = after_b.checked_sub(before_b).unwrap();

    // Virtual balance stores amounts in voucher units (24 decimals).
    // For 6-decimal tokens the normalization factor is 10^18.
    let voucher_factor = Uint256::from(10u128.pow(18));
    assert_eq!(
        refund_a,
        Uint256::from(5_000u128) * voucher_factor,
        "token0 should be fully refunded when range is below current price",
    );
    assert!(
        refund_b <= Uint256::from(5u128) * voucher_factor,
        "token1 should be nearly fully consumed in above-range add",
    );
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/one_sided.rs::clp_one_sided_wrong_token_tight_slippage_fails
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_strict_slippage_rejects_large_leftover(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 30_000, 30_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let ids_before = list_position_ids(&factory).unwrap();
    let res = add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 10_000, 1_000),
        pool_key,
        -10,
        10,
        None,
        100,
    );

    match mode {
        FactorySetupMode::Native => {
            let err = res.unwrap_err();
            assert!(
                !err.to_string().is_empty(),
                "strict slippage should reject large leftover liquidity in native mode",
            );
        }
        _ => {
            // In IBC/EVM mode, the slippage failure happens on the router side
            // and comes back as an error ack. The helper may return an error
            // (no clp_add_liquidity event emitted on failure). Either way,
            // position state should be unchanged.
            let _ = res;
            let ids_after = list_position_ids(&factory).unwrap();
            assert_eq!(
                ids_after, ids_before,
                "failed ibc/evm slippage add must not mint or finalize position state",
            );
        }
    }
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/positions.rs::clp_position_lifecycle_on_all — EVM + Cosmos
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_add_then_partial_remove_updates_position_liquidity_exactly(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let position_id = first_position_id(&factory);
    let before = position(&router, pool_key.clone(), position_id);

    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 8_000, 8_000),
        pool_key.clone(),
        before.lower_tick_index,
        before.upper_tick_index,
        Some(position_id),
        100,
    )
    .unwrap();

    let after_add = position(&router, pool_key.clone(), position_id);
    let added_liquidity = after_add.liquidity.checked_sub(before.liquidity).unwrap();
    assert!(
        added_liquidity > Uint128::zero(),
        "expected positive liquidity increase",
    );

    let remove_delta = Uint128::new((added_liquidity.u128() / 2).max(1));
    remove_concentrated_liquidity(
        &factory,
        &router,
        pool_key.clone(),
        position_id,
        remove_delta,
    )
    .unwrap();

    let after_remove = position(&router, pool_key, position_id);
    assert_eq!(
        after_remove.liquidity,
        after_add.liquidity.checked_sub(remove_delta).unwrap(),
    );
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/positions.rs::clp_position_lifecycle_on_all — EVM + Cosmos
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_remove_full_burns_position_and_owner_index(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 25_000, 25_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let position_id = first_position_id(&factory);
    let pos = position(&router, pool_key.clone(), position_id);

    remove_concentrated_liquidity(&factory, &router, pool_key, position_id, pos.liquidity).unwrap();

    let ids = list_position_ids(&factory).unwrap();
    assert!(
        !ids.iter().any(|id| id == &position_id.to_string()),
        "full remove should burn position token and owner index entry",
    );
}
