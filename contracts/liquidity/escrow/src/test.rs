#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{from_binary, to_binary, Coin, Uint128};
    use cw20::Cw20ReceiveMsg;
    use euclid::error::ContractError;
    use euclid::msgs::escrow::{AllowedTokenResponse, ExecuteMsg, InstantiateMsg, TokenIdResponse};
    use euclid::msgs::pool::Cw20HookMsg;
    use euclid::token::{Token, TokenInfo, TokenType};
    use crate::contract::{execute, instantiate};
    use crate::query::{query_token_allowed, query_token_id};
    use crate::state::{ALLOWED_DENOMS, STATE, DENOM_TO_AMOUNT};

    struct TestInstantiateMsg {
        name: &'static str,
        msg: InstantiateMsg,
        expected_error: Option<ContractError>,
    }

    struct TestExecuteMsg {
        name: &'static str,
        msg: ExecuteMsg,
        expected_error: Option<ContractError>,
    }

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let test_cases = vec![
            TestInstantiateMsg {
                name: "Valid instantiate message with allowed denom",
                msg: InstantiateMsg {
                    token_id: Token { id: "token1".to_string() },
                    allowed_denom: Some("denom1".to_string()),
                },
                expected_error: None,
            },
            TestInstantiateMsg {
                name: "Valid instantiate message without allowed denom",
                msg: InstantiateMsg {
                    token_id: Token { id: "token2".to_string() },
                    allowed_denom: None,
                },
                expected_error: None,
            },
        ];

        for test in test_cases {
            let res = instantiate(deps.as_mut(), env.clone(), info.clone(), test.msg.clone());
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => assert!(res.is_ok(), "{}", test.name),
            }
        }
    }

    #[test]
    fn test_execute_add_allowed_denom() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "token1".to_string() },
            allowed_denom: None,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

        let test_cases = vec![
            TestExecuteMsg {
                name: "Add allowed denom by factory",
                msg: ExecuteMsg::AddAllowedDenom { denom: "denom1".to_string() },
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Add duplicate denom",
                msg: ExecuteMsg::AddAllowedDenom { denom: "denom1".to_string() },
                expected_error: Some(ContractError::DuplicateDenominations {}),
            },
            TestExecuteMsg {
                name: "Add allowed denom by non-factory",
                msg: ExecuteMsg::AddAllowedDenom { denom: "denom2".to_string() },
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];

        for test in test_cases {
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-factory") {
                    mock_info("non-factory", &[])
                } else {
                    info.clone()
                },
                test.msg.clone(),
            );
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => {
                    assert!(res.is_ok(), "{}", test.name);

                    // Verify the denom was added
                    let allowed_denoms = ALLOWED_DENOMS.load(&deps.storage).unwrap();
                    assert!(allowed_denoms.contains(&"denom1".to_string()));
                }
            }
        }
    }

    #[test]
    fn test_execute_disallow_denom() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "token1".to_string() },
            allowed_denom: Some("denom1".to_string()),
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

        let test_cases = vec![
            TestExecuteMsg {
                name: "Disallow denom by factory",
                msg: ExecuteMsg::DisallowDenom { denom: "denom1".to_string() },
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Disallow non-existing denom",
                msg: ExecuteMsg::DisallowDenom { denom: "denom2".to_string() },
                expected_error: Some(ContractError::DenomDoesNotExist {}),
            },
            TestExecuteMsg {
                name: "Disallow denom by non-factory",
                msg: ExecuteMsg::DisallowDenom { denom: "denom1".to_string() },
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];

        for test in test_cases {
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-factory") {
                    mock_info("non-factory", &[])
                } else {
                    info.clone()
                },
                test.msg.clone(),
            );
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => {
                    assert!(res.is_ok(), "{}", test.name);

                    // Verify the denom was removed
                    let allowed_denoms = ALLOWED_DENOMS.load(&deps.storage).unwrap();
                    assert!(!allowed_denoms.contains(&"denom1".to_string()));
                }
            }
        }
    }
    #[test]
    fn test_execute_deposit_native() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("factory", &[Coin { denom: "denom1".to_string(), amount: Uint128::new(100) }]);
    
        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "token1".to_string() },
            allowed_denom: Some("denom1".to_string()),
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    
        let test_cases = vec![
            TestExecuteMsg {
                name: "Deposit native token by factory",
                msg: ExecuteMsg::DepositNative {},
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Deposit unsupported denom",
                msg: ExecuteMsg::DepositNative {},
                expected_error: Some(ContractError::UnsupportedDenomination {}),
            },
            TestExecuteMsg {
                name: "Deposit by non-factory",
                msg: ExecuteMsg::DepositNative {},
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];
    
        for test in test_cases {
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-factory") {
                    mock_info("non-factory", &[Coin { denom: "denom1".to_string(), amount: Uint128::new(100) }])
                } else if test.name.contains("unsupported denom") {
                    mock_info("factory", &[Coin { denom: "denom2".to_string(), amount: Uint128::new(100) }])
                } else {
                    info.clone()
                },
                test.msg.clone(),
            );
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => {
                    assert!(res.is_ok(), "{}", test.name);
    
                    // Verify the deposit was successful
                    let denom_amount = DENOM_TO_AMOUNT.load(&deps.storage, "denom1".to_string()).unwrap();
                    assert_eq!(denom_amount.amount, Uint128::new(100));
                }
            }
        }
    }
    #[test]
    fn test_receive_cw20() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);
    
        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "token1".to_string() },
            allowed_denom: Some("cw20token".to_string()),
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    
        let cw20_receive_msg = Cw20ReceiveMsg {
            sender: "creator".to_string(),
            amount: Uint128::new(100),
            msg: to_binary(&Cw20HookMsg::Deposit {}).unwrap(),
        };
    
        let test_cases = vec![
            TestExecuteMsg {
                name: "Receive CW20 tokens by factory",
                msg: ExecuteMsg::Receive(cw20_receive_msg.clone()),
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Receive CW20 tokens by non-factory",
                msg: ExecuteMsg::Receive(Cw20ReceiveMsg {
                    sender: "non-factory".to_string(),
                    amount: Uint128::new(100),
                    msg: to_binary(&Cw20HookMsg::Deposit {}).unwrap(),
                }),
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];
    
        for test in test_cases {
            dbg!(&test.name);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-factory") {
                    mock_info("non-factory", &[])
                } else {
                    info.clone()
                },
                test.msg.clone(),
            );
            dbg!(&res);
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => {
                    assert!(res.is_ok(), "{}", test.name);
    
                    // Verify the deposit was successful
                    let denom_amount = DENOM_TO_AMOUNT.load(&deps.storage, "cw20token".to_string()).unwrap();
                    dbg!(&denom_amount);
                    assert_eq!(denom_amount.amount, Uint128::new(100));
                }
            }
        }
    }
    
    #[test]
    fn test_execute_withdraw() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        env.block.chain_id = "chain-1".to_string();
    
        let info = mock_info("factory", &[Coin { denom: "denom1".to_string(), amount: Uint128::new(1000) }]);
    
        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "token1".to_string() },
            allowed_denom: Some("denom1".to_string()),
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    
        let deposit_msg = ExecuteMsg::DepositNative {};
        let deposit_res = execute(deps.as_mut(), env.clone(), info.clone(), deposit_msg).unwrap();
        dbg!(&deposit_res);
    
        let initial_denom_amount = DENOM_TO_AMOUNT.load(&deps.storage, "denom1".to_string()).unwrap();
        dbg!(&initial_denom_amount);
    
        let test_cases = vec![
            TestExecuteMsg {
                name: "Withdraw by factory",
                msg: ExecuteMsg::Withdraw { recipient: "recipient1".to_string(), amount: Uint128::new(50), chain_id: "chain-1".to_string() },
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Withdraw with insufficient funds",
                msg: ExecuteMsg::Withdraw { recipient: "recipient1".to_string(), amount: Uint128::new(2000), chain_id: "chain-1".to_string() }, // Use 2000 which exceeds the balance
                expected_error: Some(ContractError::InsufficientDeposit {}),
            },
            TestExecuteMsg {
                name: "Withdraw by non-factory",
                msg: ExecuteMsg::Withdraw { recipient: "recipient1".to_string(), amount: Uint128::new(50), chain_id: "chain-1".to_string() },
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];
    
        for test in test_cases {
            dbg!(&test.name);
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-factory") {
                    mock_info("non-factory", &[])
                } else {
                    info.clone()
                },
                test.msg.clone(),
            );
            dbg!(&res);
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => {
                    assert!(res.is_ok(), "{}", test.name);
    
                    // Verify the withdrawal was successful
                    let denom_amount = DENOM_TO_AMOUNT.load(&deps.storage, "denom1".to_string()).unwrap();
                    dbg!(&denom_amount);
                    assert_eq!(denom_amount.amount, Uint128::new(950));
                }
            }
        }
    }
    
    
    
    
  #[test]
    fn test_query_token_id() {
        let mut deps = mock_dependencies();

        let env = mock_env();
        let info = mock_info("creator", &[]);

        // Instantiate the contract with a sample token id
        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "token1".to_string() },
            allowed_denom: Some("denom1".to_string()),
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

        // Query the token id
        let res = query_token_id(deps.as_ref()).unwrap();

        // Assert the response is correct
        let expected_response = TokenIdResponse { token_id: "token1".to_string() };
        assert_eq!(to_binary(&expected_response).unwrap(), res);
    }

    #[test]
    fn test_query_token_allowed() {
        let mut deps = mock_dependencies();

        let env = mock_env();
        let info = mock_info("creator", &[]);

        // Instantiate the contract with a sample token and allowed denomination
        let instantiate_msg = InstantiateMsg {
            token_id: Token { id: "token1".to_string() },
            allowed_denom: Some("denom1".to_string()),
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

        // Test querying an allowed token with the same denomination
        let token_info = TokenInfo {
            token: Token { id: "token1".to_string() },
            token_type: TokenType::Native {
                denom: "denom1".to_string(),
            },
        };
        let res = query_token_allowed(deps.as_ref(), token_info).unwrap();
        let expected_response = AllowedTokenResponse { allowed: true };
        assert_eq!(to_binary(&expected_response).unwrap(), res);

        // Test querying a non-allowed token with a different denomination
        let token_info = TokenInfo {
            token: Token { id: "token1".to_string() },
            token_type: TokenType::Native {
                denom: "denom2".to_string(),
            },
        };
        let res = query_token_allowed(deps.as_ref(), token_info).unwrap();
        let expected_response = AllowedTokenResponse { allowed: false };
        assert_eq!(to_binary(&expected_response).unwrap(), res);
    }
}

    

