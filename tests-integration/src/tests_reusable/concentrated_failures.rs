#![cfg(not(target_arch = "wasm32"))]

use cw_orch::prelude::*;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::token::{Token, TokenType, TokenWithDenom};
use rstest::rstest;

use crate::helpers::factory::{faucet, get_position_token};
use crate::helpers::relayer::{
    extract_ack_packet_events, relay_factory_ack_packet, relay_factory_send_packet,
    relay_factory_router_factory,
};
use crate::tests_reusable::concentrated_create_pool::{pair_with_amounts, setup_concentrated_env};
use crate::tests_reusable::constants::FACTORY_CHAIN_ID_IBC;
use crate::tests_reusable::factory_register::FactorySetupMode;

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
            token.amount.u128(),
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
                lp_token_name: "LPNAME".to_string(),
                lp_token_symbol: "LPSYMBOL".to_string(),
                lp_token_decimal: 6,
                slippage_tolerance_bps: 100,
                lp_token_marketing: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap();

    let _ack_events = relay_factory_send_packet(tx.events, &router).unwrap();

    let pools = factory.get_all_concentrated_pools().unwrap().pools;
    assert!(pools.is_empty(), "pool should not finalize on factory without ack");

    let position_token = get_position_token(&factory).unwrap();
    let tokens = position_token
        .query::<position_token::msg::TokensResponse>(&position_token::msg::QueryMsg::AllTokens {})
        .unwrap()
        .tokens;
    assert!(
        tokens.is_empty(),
        "position NFT should not be minted on factory without ack",
    );
}

#[test]
fn test_ack_error_rolls_back_pending() {
    let (_interchain, factory, router, _token_a, _token_b) =
        setup_concentrated_env(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC);

    let token_x = TokenWithDenom {
        token: Token::create("conc.unregistered.x".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.unregistered.x".to_string(),
        },
    };
    let token_y = TokenWithDenom {
        token: Token::create("conc.unregistered.y".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.unregistered.y".to_string(),
        },
    };
    let pair = pair_with_amounts(&token_x, &token_y, 10_000, 10_000);

    let mut funds = vec![];
    for token in pair.get_vec_token_info() {
        faucet(
            factory.environment(),
            factory.environment().sender.as_str(),
            token.amount.u128(),
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
                lp_token_name: "LPNAME".to_string(),
                lp_token_symbol: "LPSYMBOL".to_string(),
                lp_token_decimal: 6,
                slippage_tolerance_bps: 100,
                lp_token_marketing: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap();

    let chain_uid = factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx.events, &factory, &router, &chain_uid).unwrap();

    let pools = factory.get_all_concentrated_pools().unwrap().pools;
    assert!(
        pools.is_empty(),
        "error ack must not leave concentrated pool mapping",
    );

    let position_token = get_position_token(&factory).unwrap();
    let tokens = position_token
        .query::<position_token::msg::TokensResponse>(&position_token::msg::QueryMsg::AllTokens {})
        .unwrap()
        .tokens;
    assert!(tokens.is_empty(), "error ack must not mint position NFT");
}

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
            token.amount.u128(),
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
                lp_token_name: "LPNAME".to_string(),
                lp_token_symbol: "LPSYMBOL".to_string(),
                lp_token_decimal: 6,
                slippage_tolerance_bps: 100,
                lp_token_marketing: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap();

    let chain_uid = factory.get_state().unwrap().chain_uid;
    let ack_events = relay_factory_send_packet(tx.events, &router).unwrap();
    relay_factory_ack_packet(&factory, ack_events.clone(), &chain_uid).unwrap();

    let position_token = get_position_token(&factory).unwrap();
    let tokens_after_first = position_token
        .query::<position_token::msg::TokensResponse>(&position_token::msg::QueryMsg::AllTokens {})
        .unwrap()
        .tokens;
    let pools_after_first = factory.get_all_concentrated_pools().unwrap().pools;

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
            .unwrap();
    }

    let tokens_after_second = position_token
        .query::<position_token::msg::TokensResponse>(&position_token::msg::QueryMsg::AllTokens {})
        .unwrap()
        .tokens;
    let pools_after_second = factory.get_all_concentrated_pools().unwrap().pools;

    assert_eq!(tokens_after_first, tokens_after_second);
    assert_eq!(pools_after_first, pools_after_second);
}
