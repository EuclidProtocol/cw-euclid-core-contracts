#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::factory::faucet;
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::Uint128;
use cw_orch::mock::MockBase;
use cw_orch::prelude::CwOrchError;
use cw_orch::prelude::Environment;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::ExecuteMsgFns as FactoryExecuteMsgFns;
use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::token::{
    PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
};
use factory::FactoryContract;
use router::RouterContract;
use rstest::rstest;

pub fn create_pool(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    pair_with_denom: PairWithDenomAndAmount,
    slippage_tolerance_bps: u64,
    pool_config: PoolConfig,
) -> Result<(), CwOrchError> {
    let chain = factory.environment();
    let mut funds = vec![];
    for token in pair_with_denom.get_vec_token_info() {
        faucet(
            chain,
            chain.sender.as_str(),
            token.amount.u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
    let tx_response = factory
        .request_pool_creation(
            CrossChainConfig::default(),
            6,
            "LPSYMBOL".to_string(),
            "LPSYMBOL".to_string(),
            pair_with_denom.clone(),
            pool_config,
            slippage_tolerance_bps,
            None,
            &funds.to_vec(),
        )
        .unwrap();
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{setup_interchain, setup_router};
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::tests_reusable::factory_register::setup_factory;
    use crate::tests_reusable::factory_register_denom::register_denom;
    use cosmwasm_std::Uint64;

    #[rstest]
    #[case::stable(PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, FACTORY_CHAIN_ID_LOCAL)]
    #[case::constant_product(PoolConfig::ConstantProduct {}, FACTORY_CHAIN_ID_LOCAL)]
    #[case::stable(PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, FACTORY_CHAIN_ID_IBC)]
    #[case::constant_product(PoolConfig::ConstantProduct {}, FACTORY_CHAIN_ID_IBC)]
    #[case::stable(PoolConfig::Stable { amp_factor: Some(Uint64::new(100)) }, FACTORY_CHAIN_ID_EVM)]
    #[case::constant_product(PoolConfig::ConstantProduct {}, FACTORY_CHAIN_ID_EVM)]
    fn test_create_pool(#[case] pool_config: PoolConfig, #[case] factory_chain_id: &str) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

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

        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let pair_with_denom = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token,
                token_type: token_a.token_type.clone(),
                amount: Uint128::from(10_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token,
                token_type: token_b.token_type.clone(),
                amount: Uint128::from(10_000u128),
            },
        };

        create_pool(&factory, &router, pair_with_denom.clone(), 500, pool_config).unwrap();

        let registered_pool = factory.get_vlp(pair_with_denom.get_pair().unwrap());
        assert!(registered_pool.is_ok(), "Pool not registered");
    }
}
