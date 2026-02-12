#![cfg(not(target_arch = "wasm32"))]
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::ExecuteMsgFns;
use euclid::msgs::factory::QueryMsgFns;
use euclid::token::TokenWithDenom;
use factory::FactoryContract;
use router::RouterContract;

use crate::helpers::relayer::relay_factory_router_factory;

pub fn register_denom(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let tx_response = factory.register_denom(CrossChainConfig::default(), token.clone())?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

pub fn deregister_denom(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    let tx_response = factory.deregister_denom(CrossChainConfig::default(), token.clone())?;
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::setup_interchain;
    use crate::tests_reusable::factory_register::setup_factory;
    use crate::{
        helpers::chains::setup_router,
        tests_reusable::constants::{
            FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
        },
    };
    use euclid::token::{Token, TokenType};
    use rstest::rstest;

    #[rstest]
    #[case("native", FACTORY_CHAIN_ID_LOCAL)]
    #[case("smart", FACTORY_CHAIN_ID_LOCAL)]
    #[case("native", FACTORY_CHAIN_ID_IBC)]
    #[case("smart", FACTORY_CHAIN_ID_IBC)]
    #[case("native", FACTORY_CHAIN_ID_EVM)]
    #[case("smart", FACTORY_CHAIN_ID_EVM)]
    fn test_register_denom(#[case] token_type_case: &str, #[case] factory_chain_id: &str) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let token_type = match token_type_case {
            "native" => TokenType::Native {
                denom: "eucl".to_string(),
            },
            "smart" => TokenType::Smart {
                contract_address: factory
                    .environment()
                    .addr_make("token_contract")
                    .to_string(),
            },
            _ => unreachable!("unexpected token type case"),
        };
        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type,
        };

        register_denom(&factory, &router, token.clone()).unwrap();

        let escrow_response = factory.get_escrow(token.token.to_string()).unwrap();
        assert!(
            escrow_response
                .denoms
                .iter()
                .any(|d| d == &token.token_type),
            "Escrow found but denom not registered"
        );
    }

    #[rstest]
    #[case(FACTORY_CHAIN_ID_LOCAL)]
    #[case(FACTORY_CHAIN_ID_LOCAL)]
    #[case(FACTORY_CHAIN_ID_IBC)]
    #[case(FACTORY_CHAIN_ID_IBC)]
    #[case(FACTORY_CHAIN_ID_EVM)]
    #[case(FACTORY_CHAIN_ID_EVM)]
    fn test_deregister_denom(#[case] factory_chain_id: &str) {
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
            },
        };

        register_denom(&factory, &router, token.clone()).unwrap();

        deregister_denom(&factory, &router, token.clone()).unwrap();

        let escrow_response = factory.get_escrow(token.token.to_string()).unwrap();
        assert!(!escrow_response
            .denoms
            .iter()
            .any(|d| d == &token.token_type));
    }
}
