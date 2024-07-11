use cosmwasm_std::{
    coin,
    testing::{mock_dependencies, mock_env, mock_info},
    to_json_binary, Addr, Coin, Uint128,
};

use crate::{
    contract::{execute, instantiate},
    query::query_token_id,
    state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT},
};

use euclid::{
    error::ContractError,
    msgs::escrow::{ExecuteMsg, InstantiateMsg, TokenIdResponse},
    token::{Token, TokenType},
};

fn init_escrow() {
    let mut deps = mock_dependencies();
    let info = mock_info("creator", &[]);
    let env = mock_env();

    let msg = InstantiateMsg {
        token_id: Token::create("eucl".to_string()).unwrap(),
        allowed_denom: Some(TokenType::Native {
            denom: "eucl".to_string(),
        }),
    };

    let res = instantiate(deps.as_mut(), env, info, msg);
    assert!(res.is_ok())
}

#[test]
fn test_instantiation() {
    init_escrow()
}

#[test]
fn test_deposit_native() {
    let mut deps = mock_dependencies();
    let info = mock_info("creator", &[]);
    let env = mock_env();
    let msg = InstantiateMsg {
        token_id: Token::create("eucl".to_string()).unwrap(),
        allowed_denom: Some(TokenType::Native {
            denom: "eucl".to_string(),
        }),
    };
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::DepositNative {};
    // No funds sent
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err, ContractError::InsufficientDeposit {});

    // Unauthorized sender (address that instantiated the contract is set as factory, which is the only authorized address)
    let info = mock_info("not_factory", &[coin(100_u128, "usdc")]);
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err, ContractError::Unauthorized {});

    // Send invalid denom
    let info = mock_info("creator", &[coin(100_u128, "usdc")]);
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err, ContractError::UnsupportedDenomination {});

    // Send zero amount
    let info = mock_info("creator", &[coin(0_u128, "eucl")]);
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err, ContractError::InsufficientDeposit {});

    // Should work
    let info = mock_info("creator", &[coin(10_u128, "eucl")]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    let denom_to_amount = DENOM_TO_AMOUNT
        .load(&deps.storage, "native:eucl".to_string())
        .unwrap();
    let expected_denom_to_amount = Uint128::new(10);
    assert_eq!(denom_to_amount, expected_denom_to_amount);
    // Deposit more
    let info = mock_info("creator", &[coin(10_u128, "eucl")]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap();
    let denom_to_amount = DENOM_TO_AMOUNT
        .load(&deps.storage, "native:eucl".to_string())
        .unwrap();
    let expected_denom_to_amount = Uint128::new(20);

    assert_eq!(denom_to_amount, expected_denom_to_amount);
}

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
                token_id: Token::create("token1".to_string()).unwrap(),
                allowed_denom: Some(TokenType::Native {
                    denom: "denom1".to_string(),
                }),
            },
            expected_error: None,
        },
        TestInstantiateMsg {
            name: "Valid instantiate message without allowed denom",
            msg: InstantiateMsg {
                token_id: Token::create("token2".to_string()).unwrap(),
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
        token_id: Token::create("token1".to_string()).unwrap(),
        allowed_denom: None,
    };
    instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

    let test_cases = vec![
        TestExecuteMsg {
            name: "Add allowed denom by factory",
            msg: ExecuteMsg::AddAllowedDenom {
                denom: TokenType::Native {
                    denom: "denom1".to_string(),
                },
            },
            expected_error: None,
        },
        TestExecuteMsg {
            name: "Add duplicate denom",
            msg: ExecuteMsg::AddAllowedDenom {
                denom: TokenType::Native {
                    denom: "denom1".to_string(),
                },
            },
            expected_error: Some(ContractError::DuplicateDenominations {}),
        },
        TestExecuteMsg {
            name: "Add allowed denom by non-factory",
            msg: ExecuteMsg::AddAllowedDenom {
                denom: TokenType::Native {
                    denom: "denom2".to_string(),
                },
            },
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
                assert!(allowed_denoms.contains(&TokenType::Native {
                    denom: "denom1".to_string()
                }));
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
        token_id: Token::create("token1".to_string()).unwrap(),
        allowed_denom: Some(TokenType::Native {
            denom: "denom1".to_string(),
        }),
    };
    instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

    let test_cases = vec![
        TestExecuteMsg {
            name: "Disallow denom by factory",
            msg: ExecuteMsg::DisallowDenom {
                denom: TokenType::Native {
                    denom: "denom1".to_string(),
                },
            },
            expected_error: None,
        },
        TestExecuteMsg {
            name: "Disallow non-existing denom",
            msg: ExecuteMsg::DisallowDenom {
                denom: TokenType::Native {
                    denom: "denom2".to_string(),
                },
            },
            expected_error: Some(ContractError::DenomDoesNotExist {}),
        },
        TestExecuteMsg {
            name: "Disallow denom by non-factory",
            msg: ExecuteMsg::DisallowDenom {
                denom: TokenType::Native {
                    denom: "denom1".to_string(),
                },
            },
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
                assert!(!allowed_denoms.contains(&TokenType::Native {
                    denom: "denom1".to_string(),
                }));
            }
        }
    }
}

#[test]
fn test_execute_withdraw() {
    let mut deps = mock_dependencies();
    let mut env = mock_env();
    env.block.chain_id = "chain-1".to_string();

    let info = mock_info(
        "factory",
        &[Coin {
            denom: "denom1".to_string(),
            amount: Uint128::new(1000),
        }],
    );

    let instantiate_msg = InstantiateMsg {
        token_id: Token::create("token1".to_string()).unwrap(),
        allowed_denom: Some(TokenType::Native {
            denom: "denom1".to_string(),
        }),
    };
    instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

    let deposit_msg = ExecuteMsg::DepositNative {};
    let deposit_res = execute(deps.as_mut(), env.clone(), info.clone(), deposit_msg).unwrap();
    dbg!(&deposit_res);

    let initial_denom_amount = DENOM_TO_AMOUNT
        .load(&deps.storage, "native:denom1".to_string())
        .unwrap();
    dbg!(&initial_denom_amount);

    let test_cases = vec![
        TestExecuteMsg {
            name: "Withdraw by factory",
            msg: ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recipient1".to_string()),
                amount: Uint128::new(50),
            },
            expected_error: None,
        },
        TestExecuteMsg {
            name: "Withdraw with insufficient funds",
            msg: ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recipient1".to_string()),
                amount: Uint128::new(2000),
            }, // Use 2000 which exceeds the balance
            expected_error: Some(ContractError::InsufficientDeposit {}),
        },
        TestExecuteMsg {
            name: "Withdraw by non-factory",
            msg: ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recipient1".to_string()),
                amount: Uint128::new(50),
            },
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
                let denom_amount = DENOM_TO_AMOUNT
                    .load(&deps.storage, "native:denom1".to_string())
                    .unwrap();
                dbg!(&denom_amount);
                assert_eq!(denom_amount, Uint128::new(950));
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
        token_id: Token::create("token1".to_string()).unwrap(),
        allowed_denom: Some(TokenType::Native {
            denom: "denom1".to_string(),
        }),
    };
    instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

    // Query the token id
    let res = query_token_id(deps.as_ref()).unwrap();

    // Assert the response is correct
    let expected_response = TokenIdResponse {
        token_id: "token1".to_string(),
    };
    assert_eq!(to_json_binary(&expected_response).unwrap(), res);
}

// #[test]
// fn test_query_token_allowed() {
//     let mut deps = mock_dependencies();

//     let env = mock_env();
//     let info = mock_info("creator", &[]);

//     // Instantiate the contract with a sample token and allowed denomination
//     let instantiate_msg = InstantiateMsg {
//         token_id: Token::create("token1".to_string()).unwrap(),
//         allowed_denom: Some(TokenType::Native {
//             denom: "denom1".to_string(),
//         }),
//     };
//     instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

//     // Test querying an allowed token with the same denomination
//     let token_info = TokenInfo {
//         token_id: Token::create("token1".to_string()).unwrap(),
//         token_type: TokenType::Native {
//             denom: TokenType::Native {
//                 denom: "denom1".to_string(),
//             },
//         },
//     };
//     let res = query_token_allowed(deps.as_ref(), token_info).unwrap();
//     let expected_response = AllowedTokenResponse { allowed: true };
//     assert_eq!(to_json_binary(&expected_response).unwrap(), res);

//     // Test querying a non-allowed token with a different denomination
//     let token_info = TokenInfo {
//         token: Token {
//             id: "token1".to_string(),
//         },
//         token_type: TokenType::Native {
//             denom: TokenType::Native {
//                 denom: "denom2".to_string(),
//             },
//         },
//     };
//     let res = query_token_allowed(deps.as_ref(), token_info).unwrap();
//     let expected_response = AllowedTokenResponse { allowed: false };
//     assert_eq!(to_json_binary(&expected_response).unwrap(), res);
// }
