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
