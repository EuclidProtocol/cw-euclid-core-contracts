#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::msgs::vlp::base::VlpConcentratedCollectFeesMsg;
use euclid::msgs::vlp::concentrated::msg::ExecuteMsg as ConcentratedExecuteMsg;
use euclid::voucher::BalanceKey;
use rstest::rstest;

use crate::helpers::chains::{get_concentrated_vlp, get_virtual_balance};
use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, list_position_ids,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;

fn first_position_id(factory: &factory::FactoryContract<cw_orch::mock::MockBase>) -> Uint128 {
    let ids = list_position_ids(factory).unwrap();
    Uint128::new(ids.first().unwrap().parse::<u128>().unwrap())
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/fees.rs::clp_fee_collect_pays_owner_and_is_idempotent_on_all — EVM + Cosmos
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_collect_fees_is_idempotent(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let position_id = first_position_id(&factory);

    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint256::from(20_000u128),
    );

    let sender = CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    );
    let mut vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );
    vlp.set_sender(&router.address().unwrap());

    let virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );

    let before_0 = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap()
        .amount;
    let before_1 = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token_b.token.to_string(),
        })
        .unwrap()
        .amount;

    vlp.execute(
        &ConcentratedExecuteMsg::CollectFees(VlpConcentratedCollectFeesMsg {
            sender: sender.clone(),
            tx_id: "collect_once".to_string(),
            pool_key: pool_key.clone(),
            position_id,
            recipient: sender.clone(),
        }),
        &[],
    )
    .unwrap();

    let after_first_0 = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap()
        .amount;
    let after_first_1 = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token_b.token.to_string(),
        })
        .unwrap()
        .amount;
    assert!(
        after_first_0 > before_0 || after_first_1 > before_1,
        "first collect should transfer accrued fees"
    );

    vlp.execute(
        &ConcentratedExecuteMsg::CollectFees(VlpConcentratedCollectFeesMsg {
            sender: sender.clone(),
            tx_id: "collect_twice".to_string(),
            pool_key,
            position_id,
            recipient: sender.clone(),
        }),
        &[],
    )
    .unwrap();

    let after_second_0 = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap()
        .amount;
    let after_second_1 = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender,
            token_id: token_b.token.to_string(),
        })
        .unwrap()
        .amount;
    assert_eq!(after_second_0, after_first_0);
    assert_eq!(after_second_1, after_first_1);
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/fees.rs::clp_out_of_range_position_collects_zero
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_out_of_range_position_collects_zero(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 60_000, 60_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let existing = list_position_ids(&factory).unwrap();
    add_concentrated_liquidity(
        &factory,
        &router,
        pair_with_amounts(&token_a, &token_b, 10_000, 10_000),
        pool_key.clone(),
        600,
        1_200,
        None,
        // Out-of-range mint is intentionally one-sided here.
        10_000,
    )
    .unwrap();
    let all = list_position_ids(&factory).unwrap();
    let new_position = all
        .iter()
        .find(|id| !existing.contains(id))
        .expect("new position id")
        .parse::<u128>()
        .unwrap();
    let new_position = Uint128::new(new_position);

    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint256::from(2_000u128),
    );

    let sender = CrossChainUser::new(
        factory.get_state().unwrap().chain_uid,
        factory.environment().sender.to_string(),
    );
    let mut vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );
    vlp.set_sender(&router.address().unwrap());

    let virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );
    let before = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token_a.token.to_string(),
        })
        .unwrap()
        .amount;

    vlp.execute(
        &ConcentratedExecuteMsg::CollectFees(VlpConcentratedCollectFeesMsg {
            sender,
            tx_id: "collect_out_of_range".to_string(),
            pool_key,
            position_id: new_position,
            recipient: CrossChainUser::new(
                factory.get_state().unwrap().chain_uid,
                factory.environment().sender.to_string(),
            ),
        }),
        &[],
    )
    .unwrap();

    let after = virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: CrossChainUser::new(
                factory.get_state().unwrap().chain_uid,
                factory.environment().sender.to_string(),
            ),
            token_id: token_a.token.to_string(),
        })
        .unwrap()
        .amount;
    assert_eq!(after, before);
}
