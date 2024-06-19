use cosmwasm_std::{
    coin,
    testing::{mock_dependencies, mock_env, mock_info},
    to_json_binary, Addr, Binary, CosmosMsg, IbcMsg,
};

use crate::contract::{execute, instantiate, query};

use euclid::{
    error::ContractError,
    msgs::escrow::{ExecuteMsg, InstantiateMsg, QueryMsg},
    token::Token,
};

fn init_escrow() {
    let mut deps = mock_dependencies();
    let info = mock_info("creator", &[]);
    let env = mock_env();

    let msg = InstantiateMsg {
        token_id: Token {
            id: "eucl".to_string(),
        },
        allowed_denom: Some("eucl".to_string()),
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
        token_id: Token {
            id: "eucl".to_string(),
        },
        allowed_denom: Some("eucl".to_string()),
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
    let err = execute(deps.as_mut(), env, info, msg.clone()).unwrap_err();
    assert_eq!(err, ContractError::InsufficientDeposit {});
}
