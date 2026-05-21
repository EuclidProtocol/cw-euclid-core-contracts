#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Uint128, Uint256};
use cw_orch::mock::MockBase;
use cw_orch::prelude::*;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::ExecuteMsgFns as FactoryExecuteMsgFns;
use euclid::msgs::factory::QueryMsgFns as FactoryQueryMsgFns;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::token::{
    PairWithDenomAndAmount, Token, TokenType, TokenWithDenom, TokenWithDenomAndAmount,
};
use factory::FactoryContract;
use pool_factory::PoolFactoryContract;
use router::RouterContract;

use crate::helpers::factory::faucet;
use crate::helpers::pool_factory::setup_factory_with_pool_factory;
use crate::helpers::relayer::relay_factory_router_factory;

/// Slice 1 round-trip: with pool_factory wired, a `RequestPoolCreation` call
/// against main factory delegates to pool_factory which builds the outbound
/// packet and dispatches through main factory's `ProxySendPacket`. The ack
/// is routed back to pool_factory which registers the VLP.
fn create_pool_via_delegation(
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
            Uint128::try_from(token.amount).unwrap().u128(),
            token.token_type.clone(),
            &mut funds,
        );
    }
    let tx_response = factory.request_pool_creation(
        CrossChainConfig::default(),
        6,
        "LPSYMBOL".to_string(),
        "LPNAME".to_string(),
        pair_with_denom.clone(),
        pool_config,
        slippage_tolerance_bps,
        None,
        &funds,
    )?;
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
    use crate::tests_reusable::factory_register::FactorySetupMode;
    use crate::tests_reusable::factory_register_denom::register_denom;
    use rstest::rstest;

    #[rstest]
    #[case::native(FactorySetupMode::Native, FACTORY_CHAIN_ID_LOCAL)]
    #[case::ibc(FactorySetupMode::Ibc, FACTORY_CHAIN_ID_IBC)]
    #[case::evm(FactorySetupMode::Evm, FACTORY_CHAIN_ID_EVM)]
    fn test_pool_factory_cp_create_through_delegation(
        #[case] mode: FactorySetupMode,
        #[case] factory_chain_id: &str,
    ) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();

        let (factory, pool_factory) =
            setup_factory_with_pool_factory(&interchain, factory_chain_id, &router, mode).unwrap();

        // Sanity: main factory now reports pool factory initialised.
        let pool_factory_resp = factory.query_pool_factory_address().unwrap();
        assert!(
            pool_factory_resp.initialised,
            "main factory must report pool_factory as initialised"
        );
        assert_eq!(
            pool_factory_resp
                .pool_factory_address
                .expect("address must be set"),
            pool_factory.address().unwrap(),
        );

        let token_a = TokenWithDenom {
            token: Token::create("tokena".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokena".to_string(),
                decimals: Some(6),
            },
        };
        let token_b = TokenWithDenom {
            token: Token::create("tokenb".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokenb".to_string(),
                decimals: Some(6),
            },
        };
        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let pair_with_denom = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token.clone(),
                token_type: token_a.token_type.clone(),
                amount: Uint256::from(10_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token.clone(),
                token_type: token_b.token_type.clone(),
                amount: Uint256::from(10_000u128),
            },
        };

        create_pool_via_delegation(
            &factory,
            &router,
            pair_with_denom.clone(),
            500,
            PoolConfig::ConstantProduct {},
        )
        .expect("delegated pool creation must succeed end-to-end");

        // VLP must be visible on the new pool_factory query surface.
        use euclid::msgs::pool_factory::QueryMsgFns as PoolFactoryQueryMsgFns;
        let pair = pair_with_denom.get_pair().unwrap();
        let pf_vlp = pool_factory.get_vlp(pair.clone()).unwrap();
        assert!(
            pf_vlp.vlp_address.is_some(),
            "pool_factory must record VLP after ack"
        );
    }

    /// Sanity test: with pool_factory NOT wired, the legacy in-factory code
    /// path continues to work. Guards against the gate accidentally short-
    /// circuiting fresh chains before the bootstrap call.
    #[test]
    fn test_pool_creation_legacy_path_when_pool_factory_not_set() {
        use crate::tests_reusable::factory_create_pool::create_pool;
        use crate::tests_reusable::factory_register::setup_factory;

        let sender = "sender_for_all_chains";
        let factory_chain_id = FACTORY_CHAIN_ID_LOCAL;
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let pool_factory_resp = factory.query_pool_factory_address().unwrap();
        assert!(!pool_factory_resp.initialised);

        let token_a = TokenWithDenom {
            token: Token::create("tokena".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokena".to_string(),
                decimals: Some(6),
            },
        };
        let token_b = TokenWithDenom {
            token: Token::create("tokenb".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "tokenb".to_string(),
                decimals: Some(6),
            },
        };
        register_denom(&factory, &router, token_a.clone()).unwrap();
        register_denom(&factory, &router, token_b.clone()).unwrap();

        let pair = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token_a.token,
                token_type: token_a.token_type,
                amount: Uint256::from(10_000u128),
            },
            token_2: TokenWithDenomAndAmount {
                token: token_b.token,
                token_type: token_b.token_type,
                amount: Uint256::from(10_000u128),
            },
        };
        create_pool(&factory, &router, pair, 500, PoolConfig::ConstantProduct {}).unwrap();
    }
}
