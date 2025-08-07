#![cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use concentrated_vlp::mock::{mock_concentrated_vlp, MockConcentratedVlp};
use cosmwasm_std::coin;
use cosmwasm_std::to_json_binary;
use cosmwasm_std::Decimal;
use cosmwasm_std::Uint128;
use cw_asset::AssetInfo;
use euclid::chain::ChainUid;
use euclid::chain::CrossChainUser;
use euclid::fee::DenomFees;
use euclid::fee::Fee;
use euclid::fee::TotalFees;
use euclid::msgs::concentrated_vlp::ConcentratedPoolParams;
use euclid::msgs::concentrated_vlp::GetStateResponse;
use euclid::msgs::concentrated_vlp::PairType;
use euclid::pool::PoolConfig;
use euclid::token::Pair;
use euclid::token::Token;
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::mock::mock_router;
use router::mock::MockRouter;
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
    let mut eucl = MockEuclidBuilder::new(&mut vlp, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![
            ("concentrated_vlp", mock_concentrated_vlp()),
            ("router", mock_router()),
            ("virtual_balance", mock_virtual_balance()),
        ])
        .build(&mut vlp);
    let owner = eucl.get_wallet("owner");

    let vlp_code_id = 1;
    let router_code_id = 2;
    let virtual_balance_code_id = 3;

    let mock_router = MockRouter::instantiate(
        &mut vlp,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        0,
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
    let recipient = CrossChainUser::new(chain_uid, "useraddr".to_string());

    let fee = Fee::new(1, 2, recipient);
    let factory = eucl.add_wallet(&mut vlp, "factory");

    let concentrated_vlp_params = ConcentratedPoolParams {
        amp: Decimal::one(),
        gamma: Decimal::one(),
        mid_fee: Decimal::percent(3), // 0.03
        out_fee: Decimal::percent(5), // 0.05
        fee_gamma: Decimal::one(),
        repeg_profit_threshold: Decimal::zero(),
        min_price_scale_delta: Decimal::one(),
        price_scale: Decimal::one(),
        ma_half_time: 60u64,
        track_asset_balances: Some(true),
        fee_share: None,
        allowed_xcp_profit_drop: Some(Decimal::zero()),
        xcp_profit_losses_threshold: Some(Decimal::zero()),
    };

    let mock_concentrated_vlp = MockConcentratedVlp::instantiate(
        &mut vlp,
        vlp_code_id,
        mock_router.addr().clone(),
        mock_router.addr().clone().into_string(),
        mock_virtual_balance.addr().clone().into_string(),
        fee.clone(),
        None,
        "admin".to_string(),
        PairType::Xyk {},
        vec![AssetInfo::native("1"), AssetInfo::native("2")],
        4,
        factory.to_string(),
        Some(to_json_binary(&concentrated_vlp_params).unwrap()),
    );
}
