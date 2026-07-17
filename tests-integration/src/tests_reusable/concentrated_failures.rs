#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Event, Uint128, Uint256};
use cw_orch::prelude::*;
use euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::normalize::normalize_token_to_voucher;
use euclid::swap::NextSwapPair;
use euclid::token::{Token, TokenType, TokenWithDenom};
use euclid::utils::pagination::Pagination;
use euclid_ibc::wire::envelope::make_ack_fail;
use rstest::rstest;

use crate::helpers::factory::{
    add_concentrated_liquidity, create_concentrated_pool, faucet, get_position_token,
};
use crate::helpers::relayer::{
    extract_ack_packet_events, extract_send_packet_events, relay_factory_ack_packet,
    relay_factory_send_packet,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::constants::{FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL};
use crate::tests_reusable::factory_register::FactorySetupMode;
use crate::tests_reusable::factory_swap::swap_request;

// Cross-VM coverage: none (CosmWasm-only)
#[test]
fn test_no_ack_does_not_finalize_position() {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);

    let mut funds = vec![];
    for token in pair.get_vec_token_info() {
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            Uint128::try_from(token.amount).unwrap().u128(),
            token.token_type,
            &mut funds,
        );
    }
    let tx = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestConcentratedPoolCreation {
                pair_with_denom_and_amount: pair,
                fee_tier_bps: 500,
                tick_spacing: 10,
                slippage_tolerance_bps: 100,
                initial_tick: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap();

    let _ack_events = relay_factory_send_packet(tx.events, &router).unwrap();

    let pools = factory.get_all_concentrated_pools().unwrap().pools;
    assert!(
        pools.is_empty(),
        "pool should not finalize on factory without ack"
    );

    let position_token = get_position_token(&factory).unwrap();
    let tokens = position_token
        .query::<euclid::msgs::position_token::TokensResponse>(
            &euclid::msgs::position_token::QueryMsg::AllTokens {
                pagination: Pagination::default(),
            },
        )
        .unwrap()
        .tokens;
    assert!(
        tokens.is_empty(),
        "position NFT should not be minted on factory without ack",
    );
}

// Cross-VM coverage: none (CosmWasm-only)
#[test]
fn test_ack_error_rolls_back_pending() {
    let (_interchain, factory, _router, token_a, token_b) =
        setup_concentrated_env(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC);

    let pair = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);

    let mut funds = vec![];
    for token in pair.get_vec_token_info() {
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            Uint128::try_from(token.amount).unwrap().u128(),
            token.token_type,
            &mut funds,
        );
    }

    let chain = factory.environment();
    let sender_addr = Addr::unchecked(chain.sender.as_str());
    let balance_a_before = chain
        .query_balance(&sender_addr, "conc.token.a")
        .unwrap()
        .u128();
    let balance_b_before = chain
        .query_balance(&sender_addr, "conc.token.b")
        .unwrap()
        .u128();

    let tx = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestConcentratedPoolCreation {
                pair_with_denom_and_amount: pair,
                fee_tier_bps: 500,
                tick_spacing: 10,
                slippage_tolerance_bps: 100,
                initial_tick: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap();

    // Construct synthetic error ack events instead of relaying to the router,
    // so we can test the factory's error-ack rollback path directly.
    let send_packets = extract_send_packet_events(&tx.events);
    assert!(
        !send_packets.is_empty(),
        "expected at least one send packet"
    );

    let error_ack = make_ack_fail("simulated router error".to_string()).unwrap();
    let ack_event_type = format!("wasm-{}", EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT);
    let mut fake_ack_events = vec![];
    for packet in &send_packets {
        // The single complete acknowledgement event (ports swapped for the
        // response direction). A Cosmos leg (encoding 0) renders msg and ack
        // as raw JSON text.
        fake_ack_events.push(
            Event::new(&ack_event_type)
                .add_attribute("source_port", &packet.destination_port)
                .add_attribute("destination_port", &packet.source_port)
                .add_attribute("msg", packet.msg.clone())
                .add_attribute("sequence", packet.sequence.to_string())
                .add_attribute("destination_chain_type", "cosmos")
                .add_attribute("ack", String::from_utf8(error_ack.to_vec()).unwrap())
                .add_attribute("ack_type", "error")
                .add_attribute("version", euclid_encoding::PROTOCOL_VERSION)
                .add_attribute("encoding", packet.encoding.to_string()),
        );
    }

    let chain_uid = factory.get_state().unwrap().chain_uid;
    relay_factory_ack_packet(&factory, fake_ack_events, &chain_uid).unwrap();

    let pools = factory.get_all_concentrated_pools().unwrap().pools;
    assert!(
        pools.is_empty(),
        "error ack must not leave concentrated pool mapping",
    );

    let position_token = get_position_token(&factory).unwrap();
    let tokens = position_token
        .query::<euclid::msgs::position_token::TokensResponse>(
            &euclid::msgs::position_token::QueryMsg::AllTokens {
                pagination: Pagination::default(),
            },
        )
        .unwrap()
        .tokens;
    assert!(tokens.is_empty(), "error ack must not mint position NFT");

    // Verify tokens refunded back to sender
    let balance_a_after = chain
        .query_balance(&sender_addr, "conc.token.a")
        .unwrap()
        .u128();
    let balance_b_after = chain
        .query_balance(&sender_addr, "conc.token.b")
        .unwrap()
        .u128();
    assert_eq!(
        balance_a_after, balance_a_before,
        "token_a must be refunded to sender after error ack",
    );
    assert_eq!(
        balance_b_after, balance_b_before,
        "token_b must be refunded to sender after error ack",
    );
}

// Cross-VM coverage: testing/euclid-tests/tests/protocol/concentrated/create_pool.rs::create_concentrated_pool_without_denom_errors
#[test]
fn test_two_unregistered_tokens_rejected() {
    let (_interchain, factory, _router, _token_a, _token_b) =
        setup_concentrated_env(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC);

    let token_x = TokenWithDenom {
        token: Token::create("conc.unregistered.x".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.unregistered.x".to_string(),
            decimals: Some(6),
        },
    };
    let token_y = TokenWithDenom {
        token: Token::create("conc.unregistered.y".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.unregistered.y".to_string(),
            decimals: Some(6),
        },
    };
    let pair = pair_with_amounts(&token_x, &token_y, 10_000, 10_000);

    let mut funds = vec![];
    for token in pair.get_vec_token_info() {
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            Uint128::try_from(token.amount).unwrap().u128(),
            token.token_type,
            &mut funds,
        );
    }

    let err = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestConcentratedPoolCreation {
                pair_with_denom_and_amount: pair,
                fee_tier_bps: 500,
                tick_spacing: 10,
                slippage_tolerance_bps: 100,
                initial_tick: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap_err()
        .to_string();

    assert!(
        !err.is_empty(),
        "expected concentrated pool creation with two unregistered tokens to fail",
    );
}

// Cross-VM coverage: none (CosmWasm-only)
#[rstest]
#[case(FactorySetupMode::Ibc)]
fn test_duplicate_ack_idempotent(#[case] mode: FactorySetupMode) {
    let (_interchain, mut factory, router, token_a, token_b) =
        setup_concentrated_env(mode, FACTORY_CHAIN_ID_IBC);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);

    let mut funds = vec![];
    for token in pair.get_vec_token_info() {
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            Uint128::try_from(token.amount).unwrap().u128(),
            token.token_type,
            &mut funds,
        );
    }
    let tx = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestConcentratedPoolCreation {
                pair_with_denom_and_amount: pair,
                fee_tier_bps: 500,
                tick_spacing: 10,
                slippage_tolerance_bps: 100,
                initial_tick: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap();

    let chain_uid = factory.get_state().unwrap().chain_uid;
    let ack_events = relay_factory_send_packet(tx.events, &router).unwrap();
    relay_factory_ack_packet(&factory, ack_events.clone(), &chain_uid).unwrap();

    let replay_packets = extract_ack_packet_events(&ack_events);
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
            .unwrap_err();
    }
}

// Cross-VM coverage: none (CosmWasm-only)
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_add_liquidity_invalid_tick_range_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let err_equal = add_concentrated_liquidity(
        &factory,
        &router,
        pair.clone(),
        pool_key.clone(),
        120,
        120,
        None,
        100,
    )
    .unwrap_err()
    .to_string();
    assert!(
        !err_equal.is_empty(),
        "expected add liquidity to fail when lower_tick_index == upper_tick_index",
    );

    let err_reversed =
        add_concentrated_liquidity(&factory, &router, pair, pool_key, 120, -120, None, 100)
            .unwrap_err()
            .to_string();
    assert!(
        !err_reversed.is_empty(),
        "expected add liquidity to fail when lower_tick_index > upper_tick_index",
    );
}

// Cross-VM coverage: none (CosmWasm-only)
#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_add_liquidity_misaligned_tick_spacing_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100).unwrap();

    let err = add_concentrated_liquidity(&factory, &router, pair, pool_key, -125, 125, None, 100)
        .unwrap_err()
        .to_string();
    assert!(
        !err.is_empty(),
        "expected add liquidity with non-aligned ticks to fail",
    );
}

// Cross-VM coverage: none (CosmWasm-only)
#[test]
fn test_concentrated_swap_rejects_zero_amount() {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    let err = swap_request(
        &factory,
        &router,
        token_a.clone(),
        token_b.token.clone(),
        Uint256::zero(),
        Uint256::from(1u128),
        vec![NextSwapPair {
            token_in: token_a.token.clone(),
            token_out: token_b.token.clone(),
            pool_key: Some(pool_key),
            test_fail: None,
        }],
        vec![],
        None,
    )
    .unwrap_err()
    .to_string();

    assert!(
        !err.is_empty(),
        "expected concentrated swap with zero amount to fail",
    );
}

// Cross-VM coverage: none (CosmWasm-only)
#[test]
fn test_concentrated_swap_rejects_unreachable_min_amount_out() {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL);
    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key = create_concentrated_pool(&factory, &router, pair, 500, 10, 100).unwrap();

    // min_amount_out must be in voucher units (24 decimals) since the VLP
    // operates entirely in voucher precision.
    let decimals = token_b
        .token_type
        .get_decimals()
        .expect("token should have decimals");
    let unreachable_min = normalize_token_to_voucher(Uint256::from(1_000_000_000u128), decimals)
        .expect("normalization should succeed");

    let err = swap_request(
        &factory,
        &router,
        token_a.clone(),
        token_b.token.clone(),
        Uint256::from(1_000u128),
        unreachable_min,
        vec![NextSwapPair {
            token_in: token_a.token.clone(),
            token_out: token_b.token.clone(),
            pool_key: Some(pool_key),
            test_fail: None,
        }],
        vec![],
        None,
    )
    .unwrap_err()
    .to_string();

    assert!(
        !err.is_empty(),
        "expected concentrated swap to fail when min_amount_out is unreachable",
    );
}
