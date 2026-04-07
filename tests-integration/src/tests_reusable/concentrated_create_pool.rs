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
use crate::helpers::factory::{concentrated_pool_key, create_concentrated_pool, faucet};
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
};
use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::register_denom;

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
fn test_create_two_fee_tiers_same_pair_slippage_failing(
    #[case] mode: FactorySetupMode,
    #[case] factory_chain_id: &str,
) {
    let (_interchain, factory, router, token_a, token_b) =
        setup_concentrated_env(mode, factory_chain_id);

    let pair = pair_with_amounts(&token_a, &token_b, 10_000_000, 100_000_000);
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
