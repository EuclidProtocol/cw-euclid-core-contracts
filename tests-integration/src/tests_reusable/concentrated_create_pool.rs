#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Uint128;
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::router::query::QueryMsgFns as RouterQueryMsgFns;
use euclid::token::{PairWithDenomAndAmount, Token, TokenType, TokenWithDenom};
use factory::FactoryContract;
use router::RouterContract;
use rstest::rstest;

use crate::helpers::chains::setup_router;
use crate::helpers::factory::{
    concentrated_pool_key, create_concentrated_pool, create_concentrated_pool_with_tick, faucet,
};
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
};
use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::{deregister_denom, register_denom};

pub fn setup_concentrated_env(
    mode: FactorySetupMode,
    factory_chain_id: &str,
) -> (
    MockInterchainEnv,
    FactoryContract<MockBase>,
    RouterContract<MockBase>,
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

    let token_a = TokenWithDenom {
        token: Token::create("conc.token.a".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.token.a".to_string(),
        },
    };
    let token_b = TokenWithDenom {
        token: Token::create("conc.token.b".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.token.b".to_string(),
        },
    };
    register_denom(&factory, &router, token_a.clone()).unwrap();
    register_denom(&factory, &router, token_b.clone()).unwrap();

    (interchain, factory, router, token_a, token_b)
}

pub fn pair_with_amounts(
    token_a: &TokenWithDenom,
    token_b: &TokenWithDenom,
    amount_a: u128,
    amount_b: u128,
) -> PairWithDenomAndAmount {
    PairWithDenomAndAmount {
        token_1: token_a.with_amount(Uint128::new(amount_a)),
        token_2: token_b.with_amount(Uint128::new(amount_b)),
    }
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_create_two_fee_tiers_same_pair(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let pool_key_500 = create_concentrated_pool(&factory, &router, pair.clone(), 500, 10, 100)
        .expect("500 bps pool should be created");
    let pool_key_3000 = create_concentrated_pool(&factory, &router, pair.clone(), 3_000, 60, 100)
        .expect("3000 bps pool should be created");

    let pool_500 = factory.get_concentrated_vlp(pool_key_500.clone()).unwrap();
    let pool_3000 = factory.get_concentrated_vlp(pool_key_3000.clone()).unwrap();
    assert_ne!(
        pool_500.vlp_address, pool_3000.vlp_address,
        "different fee tiers must map to different concentrated pools",
    );

    let router_500 = router.get_vlp_by_pool_key(pool_key_500).unwrap();
    let router_3000 = router.get_vlp_by_pool_key(pool_key_3000).unwrap();
    assert_ne!(router_500.vlp, router_3000.vlp);
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_create_two_fee_tiers_same_pair_with_initial_tick(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    let (amount_a, amount_b) = (10_000_000_u128, 100_000_000_u128);
    let pair = pair_with_amounts(&token_a, &token_b, amount_a, amount_b);

    // tick = floor(ln(price) / ln(1.0001)) where price = amount_token2 / amount_token1
    // Pair sorts tokens alphabetically, so token_1 < token_2.
    // price = amount_b / amount_a = 10 → tick ≈ 23025
    let price = amount_b as f64 / amount_a as f64;
    let initial_tick = (price.ln() / 1.0001_f64.ln()).floor() as i64;

    let pool_key_500 = create_concentrated_pool_with_tick(
        &factory,
        &router,
        pair.clone(),
        500,
        10,
        100,
        Some(initial_tick),
    )
    .expect("500 bps pool should be created");
    let pool_key_3000 = create_concentrated_pool_with_tick(
        &factory,
        &router,
        pair.clone(),
        3_000,
        60,
        100,
        Some(initial_tick),
    )
    .expect("3000 bps pool should be created");

    let pool_500 = factory.get_concentrated_vlp(pool_key_500.clone()).unwrap();
    let pool_3000 = factory.get_concentrated_vlp(pool_key_3000.clone()).unwrap();
    assert_ne!(
        pool_500.vlp_address, pool_3000.vlp_address,
        "different fee tiers must map to different concentrated pools",
    );

    let router_500 = router.get_vlp_by_pool_key(pool_key_500).unwrap();
    let router_3000 = router.get_vlp_by_pool_key(pool_key_3000).unwrap();
    assert_ne!(router_500.vlp, router_3000.vlp);
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_create_pool_with_unregistered_token_passes_and_creates_escrow(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, _token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    // token_c is never registered via register_denom
    let token_c = TokenWithDenom {
        token: Token::create("conc.token.c".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.token.c".to_string(),
        },
    };

    let pair = pair_with_amounts(&token_a, &token_c, 20_000, 20_000);
    let result = create_concentrated_pool(&factory, &router, pair, 500, 10, 100);
    result.expect("pool creation with only 1 unregistered token should succeed");
    let escrow = factory
        .get_escrow(token_c.token.to_string())
        .expect("escrow for the new token must be created by pool creation ACK");
    assert!(
        escrow.denoms.contains(&token_c.token_type),
        "new token denom must be registered in its escrow after pool creation",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_create_pool_with_both_tokens_unregistered_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let sender = "sender_for_all_chains";
    let mut chains = vec![(ROUTER_CHAIN_ID, sender)];
    if factory_chain_id != ROUTER_CHAIN_ID {
        chains.push((factory_chain_id, sender));
    }
    let interchain = MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
    // Intentionally do not register any tokens
    let factory = setup_factory_with_mode(&interchain, factory_chain_id, &router, mode).unwrap();

    let token_x = TokenWithDenom {
        token: Token::create("conc.token.x".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.token.x".to_string(),
        },
    };
    let token_y = TokenWithDenom {
        token: Token::create("conc.token.y".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "conc.token.y".to_string(),
        },
    };

    let pair = pair_with_amounts(&token_x, &token_y, 20_000, 20_000);
    let err = create_concentrated_pool(&factory, &router, pair, 500, 10, 100)
        .expect_err("pool creation with both tokens unregistered must fail");
    let err_str = err.root().to_string();
    assert!(
        err_str.contains("Atleast one token must already be registered"),
        "expected both-new-tokens error, got: {err_str}",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_create_pool_with_disallowed_token_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    // Deregister token_b — escrow still exists but denom is now disallowed
    deregister_denom(&factory, &router, token_b.clone()).unwrap();

    let pair = pair_with_amounts(&token_a, &token_b, 20_000, 20_000);
    let err = create_concentrated_pool(&factory, &router, pair, 500, 10, 100)
        .expect_err("pool creation with a disallowed token must be rejected on factory call");
    let err_str = err.root().to_string();
    assert!(
        err_str.contains("UnsupportedDenomination"),
        "expected UnsupportedDenomination error, got: {err_str}",
    );
}

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
fn test_create_pool_invalid_spacing_rejected(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, _router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);
    let pair = pair_with_amounts(&token_a, &token_b, 10_000, 10_000);

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

    let pool_key = concentrated_pool_key(&pair, 500, 11);
    let err = factory
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestConcentratedPoolCreation {
                pair_with_denom_and_amount: pair,
                fee_tier_bps: 500,
                tick_spacing: 11,
                slippage_tolerance_bps: 100,
                initial_tick: None,
                cross_chain_config: CrossChainConfig::default(),
            },
            &funds,
        )
        .unwrap_err()
        .to_string();

    assert!(!err.is_empty(), "expected invalid spacing request to fail");
    assert!(
        factory.get_concentrated_vlp(pool_key).is_err(),
        "invalid pool must not be created",
    );
}
