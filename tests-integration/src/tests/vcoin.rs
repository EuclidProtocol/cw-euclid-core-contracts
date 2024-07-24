#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::coin;
use euclid::msgs::vcoin::{GetStateResponse, State};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::mock::mock_router;
use router::mock::MockRouter;
use vcoin::mock::{mock_vcoin, MockVcoin};

#[test]
fn test_proper_instantiation() {
    let mut vcoin = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut vcoin, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![("vcoin", mock_vcoin()), ("router", mock_router())])
        .build(&mut vcoin);
    let owner = andr.get_wallet("owner");

    let vcoin_code_id = 1;
    let router_code_id = 2;
    let vlp_code_id = 3;

    let mock_router = MockRouter::instantiate(
        &mut vcoin,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        vcoin_code_id,
    );

    let mock_vcoin = MockVcoin::instantiate(
        &mut vcoin,
        vcoin_code_id,
        mock_router.addr().clone(),
        mock_router.addr().clone(),
        None,
    );

    let token_id_response = MockVcoin::query_state(&mock_vcoin, &mut vcoin);
    let expected_token_id = GetStateResponse {
        state: State {
            router: mock_router.addr().clone().into_string(),
            admin: mock_router.addr().to_owned(),
        },
    };
    assert_eq!(token_id_response, expected_token_id);
}
