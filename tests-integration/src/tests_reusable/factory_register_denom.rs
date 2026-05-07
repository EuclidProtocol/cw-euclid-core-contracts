#![cfg(not(target_arch = "wasm32"))]
use crate::helpers::relayer::relay_factory_router_factory;
use cosmwasm_std::Uint128;
use cw20::{Cw20Coin, MinterResponse};
use cw_orch::{mock::MockBase, prelude::*};
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::msgs::factory::ExecuteMsgFns;
use euclid::msgs::factory::QueryMsgFns;
use euclid::msgs::lp_token::msg::InstantiateMsg as LpTokenInstantiateMsg;
use euclid::token::Pair;
use euclid::token::Token;
use euclid::token::TokenType;
use euclid::token::TokenWithDenom;
use factory::FactoryContract;
use lp_token::LpTokenContract;
use router::RouterContract;

pub fn setup_smart_denom_token(chain: &MockBase, token: Token, decimals: u32) -> TokenWithDenom {
    let sender = chain.sender.to_string();
    let cw20 = LpTokenContract::new(chain.clone());
    cw20.upload().unwrap();

    let aux_token = Token::create(format!("{}.aux", token)).unwrap();
    let token_pair = Pair::new(token.clone(), aux_token).unwrap();
    cw20.instantiate(
        &LpTokenInstantiateMsg {
            name: format!("{}_cw20", token),
            symbol: "SWAPIN".to_string(),
            decimals: decimals.try_into().unwrap(),
            initial_balances: vec![Cw20Coin {
                address: sender.clone(),
                amount: Uint128::from(10u128.pow(decimals) * 1_000_000_000u128),
            }],
            mint: Some(MinterResponse {
                minter: sender,
                cap: None,
            }),
            marketing: None,
            vlp: chain.addr_make("dummy_vlp").to_string(),
            factory: chain.addr_make("dummy_factory"),
            token_pair,
        },
        None,
        &[],
    )
    .unwrap();

    TokenWithDenom {
        token,
        token_type: TokenType::Smart {
            contract_address: cw20.address().unwrap().to_string(),
            decimals: Some(decimals),
        },
    }
}

pub fn register_denom(
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    token: TokenWithDenom,
) -> Result<(), CwOrchError> {
    let factory_chain_uid = &factory.get_state().unwrap().chain_uid;
    println!("Execute Register Denom {:?}", token);
    let tx_response = factory.register_denom(CrossChainConfig::default(), token.clone())?;
    println!("Relay Register Denom");
    relay_factory_router_factory(tx_response.events, factory, router, factory_chain_uid)?;
    // Ensure the denom is registered
    let escrow_response = factory.get_escrow(token.token.to_string()).unwrap();
    assert!(
        escrow_response
            .denoms
            .iter()
            .any(|d| d == &token.token_type),
        "Escrow found but denom not registered"
    );
    println!("Register Denom Success {:?}", escrow_response);
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
    use crate::tests_reusable::factory_register::FactorySetupMode;
    use crate::{
        helpers::chains::setup_router,
        tests_reusable::constants::{
            FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
        },
    };
    use euclid::token::{Token, TokenType};
    use rstest::rstest;

    #[rstest]
    #[case("native")]
    #[case("smart")]
    fn test_register_denom(
        #[case] token_type_case: &str,
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let token_type = match token_type_case {
            "native" => TokenType::Native {
                denom: "eucl".to_string(),
                decimals: Some(18),
            },
            "smart" => {
                let smart_denom = setup_smart_denom_token(
                    &factory.environment(),
                    Token::create("eucl".to_string()).unwrap(),
                    6,
                );
                smart_denom.token_type
            }
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
    fn test_deregister_denom(
        #[values(FactorySetupMode::Native, FactorySetupMode::Ibc, FactorySetupMode::Evm)]
        mode: FactorySetupMode,
    ) {
        let factory_chain_id = mode.chain_id();
        let sender = "sender_for_all_chains";
        let interchain = setup_interchain(sender, factory_chain_id);
        let router_chain = interchain.get_chain(ROUTER_CHAIN_ID).unwrap();
        let router = setup_router(&router_chain, vec![factory_chain_id]).unwrap();
        let factory = setup_factory(&interchain, factory_chain_id, &router).unwrap();

        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
                decimals: Some(18),
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
