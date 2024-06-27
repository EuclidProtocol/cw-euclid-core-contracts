#[cfg(test)]
mod tests {
    use super::*;
    use cw20::{Cw20Coin, Cw20Contract, Cw20ExecuteMsg, Cw20ReceiveMsg};
    use euclid::msgs::pool::Cw20HookMsg;
    use cosmwasm_std::{Addr, Coin, Empty, Uint128,to_binary};
    use cw_multi_test::{App, AppBuilder, Contract, ContractWrapper, Executor};
    use euclid::{msgs::escrow::{AllowedTokenResponse, ExecuteMsg, InstantiateMsg, QueryMsg, TokenIdResponse}, token::{Token, TokenInfo, TokenType}};
    use crate::contract::{self, execute, instantiate, query, reply};

    const FACTORY: &str = "factory";

    const USER: &str = "user";
    const NATIVE_DENOM: &str = "native";
    const SUPPLY: u128 = 1_000_000;

    fn contract_escrow() -> Box<dyn Contract<Empty>> {
        Box::new(
            ContractWrapper::new_with_empty(
                crate::contract::execute,
                crate::contract::instantiate,
                crate::contract::query,
            )
            .with_reply_empty(contract::reply),
        )
    }

    fn mock_app() -> App {
        AppBuilder::new().build(|router, _, storage| {
            router
                .bank
                .init_balance(
                    storage,
                    &Addr::unchecked(USER),
                    vec![Coin {
                        denom: NATIVE_DENOM.to_string(),
                        amount: Uint128::from(SUPPLY),
                    }],
                )
                .unwrap();
        })
    }

    #[test]
    fn test_instantiate_contract() {
        let mut app = mock_app();
        let owner = Addr::unchecked("owner");

        // Register the contract code
        let code_id = app.store_code(contract_escrow());

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: Some("ucosm".to_string()),
        };

        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();

        // Query the state to verify the instantiation
        let res: TokenIdResponse = app
            .wrap()
            .query_wasm_smart(contract_addr.clone(), &QueryMsg::TokenId {})
            .unwrap();

        assert_eq!(res.token_id, "test_token".to_string());

        // Verify the allowed denom
        let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::TokenAllowed {
                token: TokenInfo {
                    token: Token {
                        id: "test_token".to_string(),
                    },
                    token_type: TokenType::Native {
                        denom: "ucosm".to_string(),
                    },
                },
            },
        ).unwrap();

        assert!(allowed_denom_response.allowed);
    }
    
    #[test]
    fn test_add_allowed_denom() {
        let mut app = mock_app();
        let owner = Addr::unchecked(FACTORY);

        // Register the contract code
        let code_id = app.store_code(contract_escrow());

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: None,
        };

        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();

        // Execute add allowed denom
        let add_denom_msg = ExecuteMsg::AddAllowedDenom {
            denom: "ucosm".to_string(),
        };
        let res = app.execute_contract(owner.clone(), contract_addr.clone(), &add_denom_msg, &[]);

        assert!(res.is_ok());

        // Query to check if the denom is added
        let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::TokenAllowed {
                token: TokenInfo {
                    token: Token {
                        id: "test_token".to_string(),
                    },
                    token_type: TokenType::Native {
                        denom: "ucosm".to_string(),
                    },
                },
            },
        ).unwrap();

        assert!(allowed_denom_response.allowed);
    }

    #[test]
    fn test_disallow_denom() {
        let mut app = mock_app();
        let owner = Addr::unchecked(FACTORY);

        // Register the contract code
        let code_id = app.store_code(contract_escrow());

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: Some("ucosm".to_string()),
        };

        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();

        // Execute disallow denom
        let disallow_denom_msg = ExecuteMsg::DisallowDenom {
            denom: "ucosm".to_string(),
        };
        let res = app.execute_contract(owner.clone(), contract_addr.clone(), &disallow_denom_msg, &[]);

        assert!(res.is_ok());

        // Query to check if the denom is removed
        let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::TokenAllowed {
                token: TokenInfo {
                    token: Token {
                        id: "test_token".to_string(),
                    },
                    token_type: TokenType::Native {
                        denom: "ucosm".to_string(),
                    },
                },
            },
        ).unwrap();

        assert!(!allowed_denom_response.allowed);
    }

    #[test]
    fn test_deposit_native() {
        let mut app = mock_app();
        let owner = Addr::unchecked(FACTORY);

        // Register the contract code
        let code_id = app.store_code(contract_escrow());

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: Some("ucosm".to_string()),
        };

        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();

        // Execute deposit native
        let deposit_msg = ExecuteMsg::DepositNative {};
        let funds = vec![Coin {
            denom: "ucosm".to_string(),
            amount: Uint128::from(1000u128),
        }];
        let res = app.execute_contract(owner.clone(), contract_addr.clone(), &deposit_msg, &funds);

        assert!(res.is_ok());

        // Query to check if the deposit is successful
        let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::TokenAllowed {
                token: TokenInfo {
                    token: Token {
                        id: "test_token".to_string(),
                    },
                    token_type: TokenType::Native {
                        denom: "ucosm".to_string(),
                    },
                },
            },
        ).unwrap();

        assert!(allowed_denom_response.allowed);
    }

    #[test]
    fn test_withdraw() {
        let mut app = mock_app();
        let owner = Addr::unchecked(FACTORY);
    
        // Register the contract code
        let code_id = app.store_code(contract_escrow());
    
        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: Some("ucosm".to_string()),
        };
    
        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();
    
        // Execute deposit native to have funds for withdrawal
        let deposit_msg = ExecuteMsg::DepositNative {};
        let funds = vec![Coin {
            denom: "ucosm".to_string(),
            amount: Uint128::from(1000u128),
        }];
        let res = app.execute_contract(owner.clone(), contract_addr.clone(), &deposit_msg, &funds);
    
        assert!(res.is_ok(), "Deposit failed: {:?}", res.err());
    
        // Execute withdraw
        let withdraw_msg = ExecuteMsg::Withdraw {
            recipient: USER.to_string(),
            amount: Uint128::from(500u128),
            chain_id: app.block_info().chain_id.to_string(),  // Ensure correct chain_id
        };
    
        let res = app.execute_contract(owner.clone(), contract_addr.clone(), &withdraw_msg, &[]);
    
        if let Err(ref error) = res {
            println!("Withdraw error: {:?}", error);
        }
    
        assert!(res.is_ok(), "Withdraw failed: {:?}", res.err());
    }

    #[test]
    fn test_query_token_id() {
        let mut app = mock_app();
        let owner = Addr::unchecked("owner");

        // Register the contract code
        let code_id = app.store_code(contract_escrow());

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: None,
        };

        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();

        // Query the state to verify the token ID
        let res: TokenIdResponse = app
            .wrap()
            .query_wasm_smart(contract_addr.clone(), &QueryMsg::TokenId {})
            .unwrap();

        assert_eq!(res.token_id, "test_token".to_string());
    }

    #[test]
    fn test_query_allowed_token() {
        let mut app = mock_app();
        let owner = Addr::unchecked("owner");

        // Register the contract code
        let code_id = app.store_code(contract_escrow());

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: Some("ucosm".to_string()),
        };

        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();

        // Verify the allowed denom
        let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::TokenAllowed {
                token: TokenInfo {
                    token: Token {
                        id: "test_token".to_string(),
                    },
                    token_type: TokenType::Native {
                        denom: "ucosm".to_string(),
                    },
                },
            },
        ).unwrap();

        assert!(allowed_denom_response.allowed);
    }

    #[test]
    fn test_query_disallowed_token() {
        let mut app = mock_app();
        let owner = Addr::unchecked("owner");

        // Register the contract code
        let code_id = app.store_code(contract_escrow());

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "test_token".to_string() },
            allowed_denom: Some("ucosm".to_string()),
        };

        let contract_addr = app
            .instantiate_contract(
                code_id,
                owner.clone(),
                &instantiate_msg,
                &[],
                "Contract",
                None,
            )
            .unwrap();

        // Verify a disallowed denom
        let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::TokenAllowed {
                token: TokenInfo {
                    token: Token {
                        id: "test_token".to_string(),
                    },
                    token_type: TokenType::Native {
                        denom: "udenom".to_string(),
                    },
                },
            },
        ).unwrap();

        assert!(!allowed_denom_response.allowed);
    }
}
