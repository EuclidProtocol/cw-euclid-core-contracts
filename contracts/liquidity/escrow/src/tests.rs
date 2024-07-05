use cosmwasm_std::{
    coin,
    testing::{mock_dependencies, mock_env, mock_info},
    Uint128,
};

use crate::{
    contract::{execute, instantiate},
    state::DENOM_TO_AMOUNT,
};

use euclid::{
    error::ContractError,
    msgs::escrow::{ExecuteMsg, InstantiateMsg},
    token::{Token, TokenType},
};

fn init_escrow() {
    let mut deps = mock_dependencies();
    let info = mock_info("creator", &[]);
    let env = mock_env();

    let msg = InstantiateMsg {
        token_id: Token::new("eucl".to_string()).unwrap(),
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
        token_id: Token::new("eucl".to_string()).unwrap(),
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
    let info = mock_info("not_factory", &[]);
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
