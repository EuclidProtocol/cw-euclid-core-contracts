#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Uint128;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::factory::msg::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::swap::NextSwapPair;
use euclid::token::{PairWithDenomAndAmount, Token, TokenType, TokenWithDenom};
use rstest::rstest;

use crate::helpers::chains::setup_router;
use crate::helpers::factory::create_pool;
use crate::tests_reusable::constants::{
    FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
};
use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
use crate::tests_reusable::factory_register_denom::register_denom;
use crate::tests_reusable::factory_swap::swap_request;

#[rstest]
#[case(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
#[case(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
#[case(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
fn test_cp_stable_regression_smoke(#[case] mode: FactorySetupMode, #[case] factory_chain_id: &str) {
    let sender = "sender_for_all_chains";
    let mut chains = vec![(ROUTER_CHAIN_ID, sender)];
    if factory_chain_id != ROUTER_CHAIN_ID {
        chains.push((factory_chain_id, sender));
    }
    let interchain = cw_orch_interchain::mock::MockInterchainEnv::new(chains);
    let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
    let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
    let factory = setup_factory_with_mode(&interchain, factory_chain_id, &router, mode).unwrap();

    let token_a = TokenWithDenom {
        token: Token::create("reg.token.a".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "reg.token.a".to_string(),
        },
    };
    let token_b = TokenWithDenom {
        token: Token::create("reg.token.b".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "reg.token.b".to_string(),
        },
    };
    let token_c = TokenWithDenom {
        token: Token::create("reg.token.c".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "reg.token.c".to_string(),
        },
    };
    let token_d = TokenWithDenom {
        token: Token::create("reg.token.d".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "reg.token.d".to_string(),
        },
    };

    for token in [token_a.clone(), token_b.clone(), token_c.clone(), token_d.clone()] {
        register_denom(&factory, &router, token).unwrap();
    }

    create_pool(
        &factory,
        &router,
        PairWithDenomAndAmount {
            token_1: token_a.with_amount(Uint128::new(30_000)),
            token_2: token_b.with_amount(Uint128::new(30_000)),
        },
        100,
        PoolConfig::ConstantProduct {},
    )
    .unwrap();

    create_pool(
        &factory,
        &router,
        PairWithDenomAndAmount {
            token_1: token_c.with_amount(Uint128::new(30_000)),
            token_2: token_d.with_amount(Uint128::new(30_000)),
        },
        100,
        PoolConfig::Stable {
            amp_factor: Some(cosmwasm_std::Uint64::new(100)),
        },
    )
    .unwrap();

    swap_request(
        &factory,
        &router,
        token_a.clone(),
        token_b.token.clone(),
        Uint128::new(1_000),
        Uint128::one(),
        vec![NextSwapPair {
            token_in: token_a.token.clone(),
            token_out: token_b.token.clone(),
            pool_key: None,
            test_fail: None,
        }],
        vec![],
        None,
    )
    .unwrap();

    assert!(factory
        .get_vlp(
            PairWithDenomAndAmount {
                token_1: token_a.with_amount(Uint128::one()),
                token_2: token_b.with_amount(Uint128::one()),
            }
            .get_pair()
            .unwrap(),
        )
        .is_ok());
    assert!(factory
        .get_vlp(
            PairWithDenomAndAmount {
                token_1: token_c.with_amount(Uint128::one()),
                token_2: token_d.with_amount(Uint128::one()),
            }
            .get_pair()
            .unwrap(),
        )
        .is_ok());
}
