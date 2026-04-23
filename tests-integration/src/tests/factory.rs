#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{coin, Addr, Uint128};
use escrow::mock::mock_escrow;
use euclid::{admin::EuclidAdmin, chain::ChainUid, msgs::factory::StateResponse};
use factory::mock::{mock_factory, MockFactory};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};

// ---------------------------------------------------------------------------
// Mock-based unit tests (no IBC, no cw-orch)
// ---------------------------------------------------------------------------

#[test]
fn test_proper_instantiation() {
    let mut factory = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut factory, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![("escrow", mock_escrow()), ("factory", mock_factory())])
        .build(&mut factory);
    let owner = andr.get_wallet("owner");

    let escrow_code_id = 1;
    let factory_code_id = 2;
    let cw20_code_id = 3;
    let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
    let router_contract = "router_contract".to_string();
    let relayer_contract = Addr::unchecked("relayer_contract");
    let rate_limit_fee_recipient = Addr::unchecked("rate_limit_fee_recipient");
    let rate_limit_fee_denom = "rate_limit_fee_denom".to_string();
    let rate_limit_free_limit = Uint128::from(10u128);

    let mock_factory = MockFactory::instantiate(
        &mut factory,
        factory_code_id,
        owner.clone(),
        router_contract.clone(),
        chain_uid.clone(),
        escrow_code_id,
        cw20_code_id,
        true,
        relayer_contract.clone(),
        rate_limit_fee_recipient,
        rate_limit_fee_denom,
        rate_limit_free_limit,
    );

    let state_response = MockFactory::query_state(&mock_factory, &factory);
    let expected_state_id = StateResponse {
        chain_uid,
        router_contract,
        relayer_contract,
        admin: EuclidAdmin::default(owner.clone()),
        escrow_code_id,
        lp_code_id: cw20_code_id,
        is_native: true,
    };
    assert_eq!(state_response, expected_state_id);
}

// ---------------------------------------------------------------------------
// Integration tests are covered by tests_reusable/factory_*.rs modules.
// ---------------------------------------------------------------------------
