#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::coin;
use euclid::msgs::virtual_balance::{GetStateResponse, State};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::mock::mock_router;
use router::mock::MockRouter;
use virtual_balance::mock::{mock_virtual_balance, MockVirtualBalance};

#[test]
fn test_proper_instantiation() {
    let mut virtual_balance = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut virtual_balance, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![
            ("virtual_balance", mock_virtual_balance()),
            ("router", mock_router()),
        ])
        .build(&mut virtual_balance);
    let owner = andr.get_wallet("owner");

    let virtual_balance_code_id = 1;
    let router_code_id = 2;
    let vlp_code_id = 3;

    let mock_router = MockRouter::instantiate(
        &mut virtual_balance,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        virtual_balance_code_id,
    );

    let mock_virtual_balance = MockVirtualBalance::instantiate(
        &mut virtual_balance,
        virtual_balance_code_id,
        mock_router.addr().clone(),
        mock_router.addr().clone(),
        None,
    );

    let token_id_response =
        MockVirtualBalance::query_state(&mock_virtual_balance, &mut virtual_balance);
    let expected_token_id = GetStateResponse {
        state: State {
            router: mock_router.addr().clone().into_string(),
            admin: mock_router.addr().to_owned(),
        },
    };
    assert_eq!(token_id_response, expected_token_id);
}
