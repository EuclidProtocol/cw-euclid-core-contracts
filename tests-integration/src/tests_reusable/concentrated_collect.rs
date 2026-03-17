#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Uint128};
use cw_orch::prelude::*;
use euclid::cross_chain_user::CrossChainUser;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::msgs::virtual_balance::msg::QueryMsgFns as VirtualBalanceQueryMsgFns;
use euclid::msgs::vlp::concentrated::msg::{
    ExecuteMsg as ConcentratedExecuteMsg, ProtocolFeesResponse, QueryMsg as ConcentratedQueryMsg,
};
use euclid::voucher::BalanceKey;
use rstest::rstest;

use crate::helpers::chains::{get_concentrated_vlp, get_virtual_balance};
use crate::helpers::factory::{
    collect_concentrated_fees, collect_concentrated_protocol_fees, create_concentrated_pool,
    list_position_ids,
};
use crate::helpers::relayer::{
    extract_ack_packet_events, relay_factory_ack_packet, relay_factory_router_factory,
    relay_factory_send_packet,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::concentrated_swap::execute_concentrated_swap;
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
    Uint128::new(ids.first().unwrap().parse::<u128>().unwrap())
}

fn voucher_balance(
    factory: &factory::FactoryContract<cw_orch::mock::MockBase>,
    router: &router::RouterContract<cw_orch::mock::MockBase>,
    token_id: &str,
) -> Uint128 {
    let virtual_balance = get_virtual_balance(
        router.environment(),
        &router.get_state().unwrap().virtual_balance_address,
    );
    virtual_balance
        .get_balance(BalanceKey {
            cross_chain_user: sender(factory),
            token_id: token_id.to_string(),
        })
        .unwrap()
        .amount
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_collect_position_fees_native_and_ibc(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let position_id = first_position_id(&factory);

    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(20_000),
    );

    let before_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let before_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

    collect_concentrated_fees(&factory, &router, pool_key, position_id, sender(&factory)).unwrap();

    let after_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let after_1 = voucher_balance(&factory, &router, &token_b.token.to_string());
    assert!(
        after_0 > before_0 || after_1 > before_1,
        "collect should increase recipient voucher balance on at least one side",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_collect_protocol_fees_admin_native_and_ibc(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 70_000, 70_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let mut vlp = get_concentrated_vlp(
        router.environment(),
        &Addr::unchecked(router.get_vlp_by_pool_key(pool_key.clone()).unwrap().vlp),
    );
    vlp.execute(
        &ConcentratedExecuteMsg::UpdateFee {
            lp_fee_bps: None,
            euclid_fee_bps: Some(500),
            recipient: None,
        },
        &[],
    )
    .unwrap();

    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(25_000),
    );

    let before_protocol: ProtocolFeesResponse =
        vlp.query(&ConcentratedQueryMsg::ProtocolFees {}).unwrap();

    let before_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let before_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

    collect_concentrated_protocol_fees(
        &factory,
        &router,
        pool_key,
        sender(&factory),
        before_protocol.amount_0.max(Uint128::new(1)),
        before_protocol.amount_1.max(Uint128::new(1)),
    )
    .unwrap();

    let after_protocol: ProtocolFeesResponse =
        vlp.query(&ConcentratedQueryMsg::ProtocolFees {}).unwrap();
    assert!(after_protocol.amount_0 <= before_protocol.amount_0);
    assert!(after_protocol.amount_1 <= before_protocol.amount_1);

    let after_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let after_1 = voucher_balance(&factory, &router, &token_b.token.to_string());
    if !before_protocol.amount_0.is_zero() || !before_protocol.amount_1.is_zero() {
        assert!(
            after_0 > before_0 || after_1 > before_1,
            "protocol collect should increase recipient voucher balance when protocol fees exist",
        );
    } else {
        assert_eq!(after_0, before_0);
        assert_eq!(after_1, before_1);
    }
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_collect_position_fees_unauthorized_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, mut factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let position_id = first_position_id(&factory);

    let unauthorized = factory.environment().addr_make("unauthorized_collector");
    factory.set_sender(&unauthorized);
    let chain_uid = factory.get_state().unwrap().chain_uid;

    let err = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
                pool_key,
                position_id,
                recipient: CrossChainUser::new(chain_uid, unauthorized.to_string()),
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap_err()
        .to_string();
    assert!(!err.is_empty(), "unauthorized collect should fail");
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_collect_protocol_fees_non_admin_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, mut factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let unauthorized = factory.environment().addr_make("unauthorized_admin");
    factory.set_sender(&unauthorized);
    let chain_uid = factory.get_state().unwrap().chain_uid;
    let err = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::CollectConcentratedProtocolFees {
                pool_key,
                recipient: CrossChainUser::new(chain_uid, unauthorized.to_string()),
                amount_0_requested: Uint128::new(1),
                amount_1_requested: Uint128::new(1),
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap_err()
        .to_string();
    assert!(!err.is_empty(), "non-admin protocol collect should fail");
}

#[test]
fn test_collect_duplicate_ack_idempotent_ibc() {
    let (_interchain, mut factory, router, token_a, token_b) =
        setup_concentrated_env(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC);
    let pair = pair_with_amounts(&token_a, &token_b, 50_000, 50_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let position_id = first_position_id(&factory);

    execute_concentrated_swap(
        &factory,
        &router,
        pool_key.clone(),
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(20_000),
    );

    let tx = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
                pool_key,
                position_id,
                recipient: sender(&factory),
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap();

    let chain_uid = factory.get_state().unwrap().chain_uid;
    let ack_events = relay_factory_send_packet(tx.events, &router).unwrap();
    relay_factory_ack_packet(&factory, ack_events.clone(), &chain_uid).unwrap();

    let first_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let first_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

    let replay_packets = extract_ack_packet_events(&ack_events);
    let user_sender = factory.environment().sender.clone();
    let relayer = factory.get_state().unwrap().relayer_contract;
    factory.set_sender(&relayer);
    for packet in replay_packets {
        factory
            .execute(
                &euclid::msgs::factory::ExecuteMsg::AcknowledgePacket {
                    source_port: packet.source_port,
                    destination_port: packet.destination_port,
                    msg: packet.msg,
                    sequence: packet.sequence,
                    ack: packet.ack,
                },
                &[],
            )
            .unwrap();
    }
    factory.set_sender(&user_sender);

    let second_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let second_1 = voucher_balance(&factory, &router, &token_b.token.to_string());
    assert_eq!(second_0, first_0);
    assert_eq!(second_1, first_1);
}

#[test]
fn test_collect_ack_error_rollback_ibc() {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC);
    let pair = pair_with_amounts(&token_a, &token_b, 40_000, 40_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();
    let chain_uid = factory.get_state().unwrap().chain_uid;
    let missing_position_id = Uint128::new(999_999_999_999u128);

    let before_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let before_1 = voucher_balance(&factory, &router, &token_b.token.to_string());

    let tx_first = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
                pool_key: pool_key.clone(),
                position_id: missing_position_id,
                recipient: sender(&factory),
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap();
    relay_factory_router_factory(tx_first.events, &factory, &router, &chain_uid).unwrap();

    let tx_second = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::CollectConcentratedFees {
                pool_key,
                position_id: missing_position_id,
                recipient: sender(&factory),
                cross_chain_config: CrossChainConfig::default(),
            },
            &[],
        )
        .unwrap();
    relay_factory_router_factory(tx_second.events, &factory, &router, &chain_uid).unwrap();

    let after_0 = voucher_balance(&factory, &router, &token_a.token.to_string());
    let after_1 = voucher_balance(&factory, &router, &token_b.token.to_string());
    assert_eq!(after_0, before_0, "error ack must not move balance");
    assert_eq!(after_1, before_1, "error ack must not move balance");
}
