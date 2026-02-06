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
    use crate::helpers::chains::setup_router;
    use crate::tests_reusable::factory_register::setup_factory;
    use cw_orch_interchain::mock::MockInterchainEnv;
    use euclid::token::{Token, TokenType};

    #[test]
    fn test_register_denom() {
        let sender = "sender_for_all_chains";
        let factory_chain_id = "nibiru";
        let router_chain_id = "nibiru";
        let interchain = MockInterchainEnv::new(vec![(router_chain_id, sender)]);
        let router_chain = interchain.get_chain(router_chain_id).unwrap();
        let router = setup_router(&router_chain).unwrap();
        let factory =
            setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
            },
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

    #[test]
    fn test_deregister_denom() {
        let sender = "sender_for_all_chains";
        let factory_chain_id = "nibiru";
        let router_chain_id = "nibiru";
        let interchain = MockInterchainEnv::new(vec![(router_chain_id, sender)]);
        let router_chain = interchain.get_chain(router_chain_id).unwrap();
        let router = setup_router(&router_chain).unwrap();
        let factory =
            setup_factory(&interchain, factory_chain_id, router_chain_id, &router).unwrap();

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
