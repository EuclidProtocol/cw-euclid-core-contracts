#![cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use cosmwasm_std::coin;
use cosmwasm_std::Addr;
use cosmwasm_std::Uint128;
use cp_vlp::mock::mock_cp_vlp;
use cp_vlp::mock::MockCpVlp;
use euclid::admin::EuclidAdmin;
use euclid::chain::ChainUid;
use euclid::cross_chain_user::CrossChainUser;
use euclid::fee::DenomFees;
use euclid::fee::Fee;
use euclid::fee::TotalFees;
use euclid::msgs::vlp::base::PoolConfig;
use euclid::msgs::vlp::cp::GetStateResponse;
use euclid::token::Pair;
use euclid::token::Token;
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::mock::mock_router;
use router::mock::MockRouter;
use stable_vlp::mock::mock_stable_vlp;
use virtual_balance::mock::mock_virtual_balance;
use virtual_balance::mock::MockVirtualBalance;

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
            ("router", mock_router()),
            ("virtual_balance", mock_virtual_balance()),
            ("vlp", mock_cp_vlp()),
            ("stable_vlp", mock_stable_vlp()),
        ])
        .build(&mut vlp);
    let owner = andr.get_wallet("owner");

    let router_code_id = 1;
    let virtual_balance_code_id = 2;
    let vlp_code_id = 3;
    let stable_vlp_code_id = 4;

    let mock_router = MockRouter::instantiate(
        &mut vlp,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        stable_vlp_code_id,
        virtual_balance_code_id,
        Addr::unchecked("relayer_contract"),
        Addr::unchecked("release_fee_recipient"),
        Addr::unchecked("default_fee_recipient"),
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
    let recipient = CrossChainUser::new(chain_uid, "useraddr".to_string());

    let fee = Fee::new(1, 2, recipient);

    let admin = EuclidAdmin::default(owner.clone());

    let mock_vlp = MockCpVlp::instantiate(
        &mut vlp,
        vlp_code_id,
        mock_router.addr().clone(),
        mock_router.addr().clone(),
        mock_virtual_balance.addr().clone(),
        pair.clone(),
        fee.clone(),
        None,
        admin.clone(),
    );

    let token_id_response = MockCpVlp::query_state(&mock_vlp, &vlp);
    let expected_token_id = GetStateResponse {
        pair,
        router: mock_router.addr().clone(),
        virtual_balance_contract: mock_virtual_balance.addr().clone(),
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
        pool_config: PoolConfig::ConstantProduct {},
    };
    assert_eq!(token_id_response, expected_token_id);

    let admin_response = MockCpVlp::query_admin(&mock_vlp, &vlp);
    assert_eq!(admin_response, admin);
}
