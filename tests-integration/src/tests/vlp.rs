#![cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use cosmwasm_std::coin;
use cosmwasm_std::Uint128;
use euclid::chain::ChainUid;
use euclid::chain::CrossChainUser;
use euclid::fee::DenomFees;
use euclid::fee::Fee;
use euclid::fee::TotalFees;
use euclid::msgs::vlp::GetStateResponse;
use euclid::token::Pair;
use euclid::token::Token;
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::mock::mock_router;
use router::mock::MockRouter;
use virtual_balance::mock::mock_virtual_balance;
use virtual_balance::mock::MockVirtualBalance;
use vlp::mock::{mock_vlp, MockVlp};

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;

#[test]
fn test_proper_instantiation() {
    let mut vlp = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut vlp, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![
            ("vlp", mock_vlp()),
            ("router", mock_router()),
            ("virtual_balance", mock_virtual_balance()),
        ])
        .build(&mut vlp);
    let owner = andr.get_wallet("owner");

    let vlp_code_id = 1;
    let router_code_id = 2;
    let virtual_balance_code_id = 3;

    let mock_router = MockRouter::instantiate(
        &mut vlp,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        virtual_balance_code_id,
    );

    let mock_virtual_balance = MockVirtualBalance::instantiate(
        &mut vlp,
        virtual_balance_code_id,
        mock_router.addr().clone(),
        mock_router.addr().clone(),
        None,
    );

    let pair = Pair::new(
        Token::create("1".to_string()).unwrap(),
        Token::create("2".to_string()).unwrap(),
    )
    .unwrap();
    let chain_uid = ChainUid::create("1".to_string()).unwrap();
    let recipient = CrossChainUser {
        chain_uid,
        address: "useraddr".to_string(),
    };

    let fee = Fee {
        lp_fee_bps: 1,
        euclid_fee_bps: 2,
        recipient,
    };

    let mock_vlp = MockVlp::instantiate(
        &mut vlp,
        vlp_code_id,
        mock_router.addr().clone(),
        mock_router.addr().clone().into_string(),
        mock_virtual_balance.addr().clone().into_string(),
        pair.clone(),
        fee.clone(),
        None,
        "admin".to_string(),
    );

    let token_id_response = MockVlp::query_state(&mock_vlp, &mut vlp);
    let expected_token_id = GetStateResponse {
        pair,
        router: mock_router.addr().clone().into_string(),
        virtual_balance: mock_virtual_balance.addr().clone().into_string(),
        fee,
        total_fees_collected: TotalFees {
            lp_fees: DenomFees {
                totals: HashMap::new(),
            },
            euclid_fees: DenomFees {
                totals: HashMap::new(),
            },
        },
        last_updated: 0,
        total_lp_tokens: Uint128::zero(),
        admin: "admin".to_string(),
    };
    assert_eq!(token_id_response, expected_token_id);
}
