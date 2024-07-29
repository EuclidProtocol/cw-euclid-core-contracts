#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{coin, Addr};
use euclid::msgs::router::StateResponse;
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};

use router::mock::{mock_router, MockRouter};
use vcoin::mock::mock_vcoin;
use vlp::mock::mock_vlp;

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;

#[test]
fn test_proper_instantiation() {
    let mut router = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut router, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![
            ("router", mock_router()),
            ("vlp", mock_vlp()),
            ("vcoin", mock_vcoin()),
        ])
        .build(&mut router);
    let owner = andr.get_wallet("owner");
    let _recipient_1 = andr.get_wallet("recipient1");
    let _recipient_2 = andr.get_wallet("recipient2");

    let router_code_id = 1;
    let vlp_code_id = 2;
    let vcoin_code_id = 3;

    let mock_router = MockRouter::instantiate(
        &mut router,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        vcoin_code_id,
    );

    let state = MockRouter::query_state(&mock_router, &mut router);
    let expected_state_response = StateResponse {
        admin: owner.clone().into_string(),
        vlp_code_id,
        vcoin_address: Some(Addr::unchecked(
            "eucl1hrpna9v7vs3stzyd4z3xf00676kf78zpe2u5ksvljswn2vnjp3ys8rp88c",
        )),
        locked: false,
    };
    assert_eq!(state, expected_state_response);
}
