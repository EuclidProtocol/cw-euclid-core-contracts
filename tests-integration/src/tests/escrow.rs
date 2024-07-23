#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::coin;
use escrow::mock::{mock_escrow, MockEscrow};
use euclid::{
    chain::ChainUid,
    msgs::escrow::TokenIdResponse,
    token::{Token, TokenType},
};
use factory::mock::mock_factory;
use factory::mock::MockFactory;
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;

#[test]
fn test_proper_instantiation() {
    let mut escrow = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut escrow, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![("escrow", mock_escrow()), ("factory", mock_factory())])
        .build(&mut escrow);
    let owner = andr.get_wallet("owner");

    let escrow_code_id = 1;
    let factory_code_id = 2;
    let cw20_code_id = 3;
    let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
    let router_contract = "router_contract".to_string();

    let token_id = Token::create("token1".to_string()).unwrap();
    let allowed_denom = Some(TokenType::Native {
        denom: "eucl".to_string(),
    });

    let mock_factory = MockFactory::instantiate(
        &mut escrow,
        factory_code_id,
        owner.clone(),
        router_contract,
        chain_uid,
        escrow_code_id,
        cw20_code_id,
    );

    let mock_escrow = MockEscrow::instantiate(
        &mut escrow,
        escrow_code_id,
        mock_factory.addr().clone(),
        token_id.clone(),
        allowed_denom,
    );

    let token_id_response = MockEscrow::query_token_id(&mock_escrow, &mut escrow);
    let expected_token_id = TokenIdResponse {
        token_id: token_id.to_string(),
    };
    assert_eq!(token_id_response, expected_token_id);
}

//     #[test]
//     fn test_add_allowed_denom() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked(FACTORY);

//         // Register the contract code
//         let code_id = app.store_code(contract_escrow());

//         let instantiate_msg = InstantiateMsg {
//             token_id: Token { id: "test_token".to_string() },
//             allowed_denom: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Execute add allowed denom
//         let add_denom_msg = ExecuteMsg::AddAllowedDenom {
//             denom: "ucosm".to_string(),
//         };
//         let res = app.execute_contract(owner.clone(), contract_addr.clone(), &add_denom_msg, &[]);

//         assert!(res.is_ok());

//         // Query to check if the denom is added
//         let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
//             contract_addr.clone(),
//             &QueryMsg::TokenAllowed {
//                 token: TokenInfo {
//                     token: Token {
//                         id: "test_token".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "ucosm".to_string(),
//                     },
//                 },
//             },
//         ).unwrap();

//         assert!(allowed_denom_response.allowed);
//     }

//     #[test]
//     fn test_disallow_denom() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked(FACTORY);

//         // Register the contract code
//         let code_id = app.store_code(contract_escrow());

//         let instantiate_msg = InstantiateMsg {
//             token_id: Token { id: "test_token".to_string() },
//             allowed_denom: Some("ucosm".to_string()),
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Execute disallow denom
//         let disallow_denom_msg = ExecuteMsg::DisallowDenom {
//             denom: "ucosm".to_string(),
//         };
//         let res = app.execute_contract(owner.clone(), contract_addr.clone(), &disallow_denom_msg, &[]);

//         assert!(res.is_ok());

//         // Query to check if the denom is removed
//         let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
//             contract_addr.clone(),
//             &QueryMsg::TokenAllowed {
//                 token: TokenInfo {
//                     token: Token {
//                         id: "test_token".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "ucosm".to_string(),
//                     },
//                 },
//             },
//         ).unwrap();

//         assert!(!allowed_denom_response.allowed);
//     }

//     #[test]
//     fn test_deposit_native() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked(FACTORY);

//         // Register the contract code
//         let code_id = app.store_code(contract_escrow());

//         let instantiate_msg = InstantiateMsg {
//             token_id: Token { id: "test_token".to_string() },
//             allowed_denom: Some("ucosm".to_string()),
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Execute deposit native
//         let deposit_msg = ExecuteMsg::DepositNative {};
//         let funds = vec![Coin {
//             denom: "ucosm".to_string(),
//             amount: Uint128::from(1000u128),
//         }];
//         let res = app.execute_contract(owner.clone(), contract_addr.clone(), &deposit_msg, &funds);

//         assert!(res.is_ok());

//         // Query to check if the deposit is successful
//         let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
//             contract_addr.clone(),
//             &QueryMsg::TokenAllowed {
//                 token: TokenInfo {
//                     token: Token {
//                         id: "test_token".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "ucosm".to_string(),
//                     },
//                 },
//             },
//         ).unwrap();

//         assert!(allowed_denom_response.allowed);
//     }

//     #[test]
//     fn test_withdraw() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked(FACTORY);

//         // Register the contract code
//         let code_id = app.store_code(contract_escrow());

//         let instantiate_msg = InstantiateMsg {
//             token_id: Token { id: "test_token".to_string() },
//             allowed_denom: Some("ucosm".to_string()),
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Execute deposit native to have funds for withdrawal
//         let deposit_msg = ExecuteMsg::DepositNative {};
//         let funds = vec![Coin {
//             denom: "ucosm".to_string(),
//             amount: Uint128::from(1000u128),
//         }];
//         let res = app.execute_contract(owner.clone(), contract_addr.clone(), &deposit_msg, &funds);

//         assert!(res.is_ok(), "Deposit failed: {:?}", res.err());

//         // Execute withdraw
//         let withdraw_msg = ExecuteMsg::Withdraw {
//             recipient: USER.to_string(),
//             amount: Uint128::from(500u128),
//             chain_id: app.block_info().chain_id.to_string(),  // Ensure correct chain_id
//         };

//         let res = app.execute_contract(owner.clone(), contract_addr.clone(), &withdraw_msg, &[]);

//         if let Err(ref error) = res {
//             println!("Withdraw error: {:?}", error);
//         }

//         assert!(res.is_ok(), "Withdraw failed: {:?}", res.err());
//     }

//     #[test]
//     fn test_query_token_id() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract_escrow());

//         let instantiate_msg = InstantiateMsg {
//             token_id: Token { id: "test_token".to_string() },
//             allowed_denom: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Query the state to verify the token ID
//         let res: TokenIdResponse = app
//             .wrap()
//             .query_wasm_smart(contract_addr.clone(), &QueryMsg::TokenId {})
//             .unwrap();

//         assert_eq!(res.token_id, "test_token".to_string());
//     }

//     #[test]
//     fn test_query_allowed_token() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract_escrow());

//         let instantiate_msg = InstantiateMsg {
//             token_id: Token { id: "test_token".to_string() },
//             allowed_denom: Some("ucosm".to_string()),
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Verify the allowed denom
//         let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
//             contract_addr.clone(),
//             &QueryMsg::TokenAllowed {
//                 token: TokenInfo {
//                     token: Token {
//                         id: "test_token".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "ucosm".to_string(),
//                     },
//                 },
//             },
//         ).unwrap();

//         assert!(allowed_denom_response.allowed);
//     }

//     #[test]
//     fn test_query_disallowed_token() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract_escrow());

//         let instantiate_msg = InstantiateMsg {
//             token_id: Token { id: "test_token".to_string() },
//             allowed_denom: Some("ucosm".to_string()),
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Verify a disallowed denom
//         let allowed_denom_response: AllowedTokenResponse = app.wrap().query_wasm_smart(
//             contract_addr.clone(),
//             &QueryMsg::TokenAllowed {
//                 token: TokenInfo {
//                     token: Token {
//                         id: "test_token".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "udenom".to_string(),
//                     },
//                 },
//             },
//         ).unwrap();

//         assert!(!allowed_denom_response.allowed);
//     }
// }
