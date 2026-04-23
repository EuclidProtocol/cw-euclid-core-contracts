#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::Uint128;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::token::PairWithDenomAndAmount;

use crate::helpers::factory::create_pool as helpers_create_pool;
use crate::helpers::multi_chain::MultiChainEnv;

pub fn create_pool(
    factory_addr: &cosmwasm_std::Addr,
    factory_chain_id: &str,
    router_addr: &cosmwasm_std::Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    pool_config: PoolConfig,
) -> Result<(), anyhow::Error> {
    helpers_create_pool(
        factory_addr,
        factory_chain_id,
        router_addr,
        router_chain_id,
        env,
        pair_with_denom,
        slippage_tolerance_bps,
        pool_config,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{setup_interchain, setup_router};
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
    use crate::tests_reusable::factory_register_denom::register_denom;
    use cosmwasm_std::Uint64;
    use euclid::token::{Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount};
    use rstest::rstest;

    fn mode_for(factory_chain_id: &str) -> FactorySetupMode {
        if factory_chain_id == ROUTER_CHAIN_ID {
            FactorySetupMode::Native
        } else if factory_chain_id == FACTORY_CHAIN_ID_EVM {
            FactorySetupMode::Evm
        } else {
            FactorySetupMode::Ibc
        }
    }

    #[rstest]
    #[case::stable(PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, FACTORY_CHAIN_ID_LOCAL)]
    #[case::constant_product(PoolConfig::ConstantProduct {}, FACTORY_CHAIN_ID_LOCAL)]
    #[case::stable(PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, FACTORY_CHAIN_ID_IBC)]
    #[case::constant_product(PoolConfig::ConstantProduct {}, FACTORY_CHAIN_ID_IBC)]
    #[case::stable(PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, FACTORY_CHAIN_ID_EVM)]
    #[case::constant_product(PoolConfig::ConstantProduct {}, FACTORY_CHAIN_ID_EVM)]
    fn test_create_pool(#[case] pool_config: PoolConfig, #[case] factory_chain_id: &str) {
        let sender = "sender_for_all_chains";
        let mut env = setup_interchain(sender, factory_chain_id);
        let router_addr =
            setup_router(env.chain_mut(ROUTER_CHAIN_ID), vec![factory_chain_id]).unwrap();
        let factory_addr = setup_factory_with_mode(
            &mut env,
            factory_chain_id,
            ROUTER_CHAIN_ID,
            &router_addr,
            mode_for(factory_chain_id),
        )
        .unwrap();

        let token_a = TokenWithDenom {
            token: Token::create("tokena".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokena".to_string(),
            },
        };
        let token_b = TokenWithDenom {
            token: Token::create("tokenb".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokenb".to_string(),
            },
        };

        register_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token_a.clone(),
        )
        .unwrap();
        register_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token_b.clone(),
        )
        .unwrap();

        let pair_with_denom = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint128::from(10_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint128::from(10_000u128),
            },
        };

        create_pool(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            pair_with_denom.clone(),
            500,
            pool_config,
        )
        .unwrap();

        let registered_pool: Result<euclid::msgs::factory::GetVlpResponse, _> =
            env.chain(factory_chain_id).try_query(
                &factory_addr,
                &euclid::msgs::factory::QueryMsg::GetVlp {
                    pair: pair_with_denom.get_pair().unwrap(),
                },
            );
        assert!(registered_pool.is_ok(), "Pool not registered");
    }
}
