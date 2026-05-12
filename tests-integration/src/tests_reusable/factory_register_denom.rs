#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::Addr;
use euclid::msgs::cross_chain_config::CrossChainConfig;
use euclid::token::TokenWithDenom;

use crate::helpers::multi_chain::MultiChainEnv;
use crate::helpers::relayer::relay_factory_router_factory;

pub fn register_denom(
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    token: TokenWithDenom,
) -> Result<(), anyhow::Error> {
    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse = env
            .chain(factory_chain_id)
            .query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        factory_state.chain_uid
    };

    println!("Execute Register Denom {:?}", token);
    let sender = env.chain(factory_chain_id).sender();
    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::RegisterDenom {
            token_with_denom: token.clone(),
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    );

    println!("Relay Register Denom");
    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;

    let escrow_response: euclid::msgs::factory::GetEscrowResponse =
        env.chain(factory_chain_id).query(
            factory_addr,
            &euclid::msgs::factory::QueryMsg::GetEscrow {
                token_id: token.token.to_string(),
            },
        );
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
    factory_addr: &Addr,
    factory_chain_id: &str,
    router_addr: &Addr,
    router_chain_id: &str,
    env: &mut MultiChainEnv,
    token: TokenWithDenom,
) -> Result<(), anyhow::Error> {
    let factory_chain_uid = {
        let factory_state: euclid::msgs::factory::StateResponse = env
            .chain(factory_chain_id)
            .query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
        factory_state.chain_uid
    };

    let sender = env.chain(factory_chain_id).sender();
    let tx_response = env.chain_mut(factory_chain_id).execute(
        &sender,
        factory_addr,
        &euclid::msgs::factory::ExecuteMsg::DeregisterDenom {
            token_with_denom: token.clone(),
            cross_chain_config: CrossChainConfig::default(),
        },
        &[],
    );

    relay_factory_router_factory(
        tx_response.events,
        factory_chain_id,
        factory_addr,
        &factory_chain_uid,
        router_chain_id,
        router_addr,
        env,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::chains::{setup_interchain, setup_router};
    use crate::tests_reusable::constants::{
        FACTORY_CHAIN_ID_EVM, FACTORY_CHAIN_ID_IBC, FACTORY_CHAIN_ID_LOCAL, ROUTER_CHAIN_ID,
    };
    use crate::helpers::chains::lp_token_code;
    use crate::tests_reusable::factory_register::{setup_factory_with_mode, FactorySetupMode};
    use cosmwasm_std::Uint128;
    use euclid::cw20_types::{Cw20Coin, MinterResponse};
    use euclid::msgs::lp_token::msg::InstantiateMsg as LpTokenInstantiateMsg;
    use euclid::token::{Pair, Token, TokenType};
    use rstest::rstest;

    fn deploy_lp_token(
        env: &mut MultiChainEnv,
        factory_chain_id: &str,
        token: &Token,
    ) -> Addr {
        let app = env.chain_mut(factory_chain_id);
        let sender = app.sender();
        let code_id = lp_token_code(app);
        let aux_token = Token::create(format!("{}.aux", token)).unwrap();
        let token_pair = Pair::new(token.clone(), aux_token).unwrap();
        app.instantiate(
            code_id,
            &sender,
            &LpTokenInstantiateMsg {
                name: format!("{}_cw20", token),
                symbol: "TEST".to_string(),
                decimals: 6,
                initial_balances: vec![Cw20Coin {
                    address: sender.to_string(),
                    amount: Uint128::new(1_000_000_000),
                }],
                mint: Some(MinterResponse {
                    minter: sender.to_string(),
                    cap: None,
                }),
                marketing: None,
                vlp: app.addr_make("dummy_vlp").to_string(),
                factory: app.addr_make("dummy_factory"),
                token_pair,
            },
            &[],
            "lp_smart_token",
        )
    }

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
    #[case("native", FACTORY_CHAIN_ID_LOCAL)]
    #[case("smart", FACTORY_CHAIN_ID_LOCAL)]
    #[case("native", FACTORY_CHAIN_ID_IBC)]
    #[case("smart", FACTORY_CHAIN_ID_IBC)]
    #[case("native", FACTORY_CHAIN_ID_EVM)]
    #[case("smart", FACTORY_CHAIN_ID_EVM)]
    fn test_register_denom(#[case] token_type_case: &str, #[case] factory_chain_id: &str) {
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

        let token_type = match token_type_case {
            "native" => TokenType::Native {
                denom: "eucl".to_string(),
            decimals: Some(6),
        },
            "smart" => {
                let token = Token::create("eucl".to_string()).unwrap();
                let lp_addr = deploy_lp_token(&mut env, factory_chain_id, &token);
                TokenType::Smart {
                    contract_address: lp_addr.to_string(),
                    decimals: Some(6),
                }
            }
            _ => unreachable!("unexpected token type case"),
        };
        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: token_type.clone(),
        };

        register_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token.clone(),
        )
        .unwrap();

        let escrow_response: euclid::msgs::factory::GetEscrowResponse =
            env.chain(factory_chain_id).query(
                &factory_addr,
                &euclid::msgs::factory::QueryMsg::GetEscrow {
                    token_id: token.token.to_string(),
                },
            );
        assert!(
            escrow_response.denoms.iter().any(|d| d == &token_type),
            "Escrow found but denom not registered"
        );
    }

    #[rstest]
    #[case(FACTORY_CHAIN_ID_LOCAL)]
    #[case(FACTORY_CHAIN_ID_IBC)]
    #[case(FACTORY_CHAIN_ID_EVM)]
    fn test_deregister_denom(#[case] factory_chain_id: &str) {
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

        let token = TokenWithDenom {
            token: Token::create("eucl".to_string()).unwrap(),
            token_type: TokenType::Native {
                denom: "eucl".to_string(),
            decimals: Some(6),
        },
        };

        register_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token.clone(),
        )
        .unwrap();
        deregister_denom(
            &factory_addr,
            factory_chain_id,
            &router_addr,
            ROUTER_CHAIN_ID,
            &mut env,
            token.clone(),
        )
        .unwrap();

        let escrow_response: euclid::msgs::factory::GetEscrowResponse =
            env.chain(factory_chain_id).query(
                &factory_addr,
                &euclid::msgs::factory::QueryMsg::GetEscrow {
                    token_id: token.token.to_string(),
                },
            );
        assert!(!escrow_response
            .denoms
            .iter()
            .any(|d| d == &token.token_type));
    }
}
