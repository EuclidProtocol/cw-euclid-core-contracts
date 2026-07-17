#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::coin;
use cosmwasm_std::{Uint128, Uint256};
use cw_multi_test::Executor;
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    fee::{DenomFees, Fee, TotalFees},
    msgs::vlp::{
        base::{PoolConfig, PoolKey, PoolType, VlpConcentratedRegisterPoolMsg},
        concentrated::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg},
    },
    token::{Pair, Token},
};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};

use crate::{
    contract::amounts_for_position_liquidity_with_bound, math::tick_math::get_sqrt_ratio_at_tick,
    mock::mock_concentrated_vlp,
};

#[test]
fn test_proper_instantiation() {
    let mut app = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut app, "admin")
        .with_wallets(vec![("owner", vec![coin(1000, "eucl")])])
        .with_contracts(vec![("concentrated_vlp", mock_concentrated_vlp())])
        .build(&mut app);

    let owner = andr.get_wallet("owner");

    let pair = Pair::new(
        Token::create("tokena".to_string()).unwrap(),
        Token::create("tokenb".to_string()).unwrap(),
    )
    .unwrap();

    let fee = Fee::new(
        10,
        10,
        CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "recipient".to_string()),
    );

    let contract = app
        .instantiate_contract(
            1,
            owner.clone(),
            &InstantiateMsg {
                virtual_balance_contract: owner.clone(),
                pair: pair.clone(),
                fee: fee.clone(),
                execute: Some(ExecuteMsg::RegisterPool(VlpConcentratedRegisterPoolMsg {
                    sender: CrossChainUser::new(
                        ChainUid::create("chain1".to_string()).unwrap(),
                        "user".to_string(),
                    ),
                    pool_key: PoolKey {
                        pair: pair.clone(),
                        pool_type: PoolType::Concentrated {
                            fee_tier_bps: 500,
                            tick_spacing: 10,
                        },
                    },
                    tx_id: "tx1".to_string(),
                })),
                admin: owner.clone(),
                fee_tier_bps: 500,
                tick_spacing: 10,
                initial_tick: None,
            },
            &[],
            "Concentrated VLP",
            None,
        )
        .unwrap();

    let state: GetStateResponse = app
        .wrap()
        .query_wasm_smart(contract, &QueryMsg::State {})
        .unwrap();

    assert_eq!(state.pair, pair);
    assert_eq!(state.router, owner);
    assert_eq!(state.fee, fee);
    assert_eq!(
        state.pool_config,
        PoolConfig::Concentrated {
            fee_tier_bps: 500,
            tick_spacing: 10
        }
    );
    assert_eq!(
        state.total_fees_collected,
        TotalFees {
            lp_fees: DenomFees {
                totals: Default::default()
            },
            euclid_fees: DenomFees {
                totals: Default::default()
            }
        }
    );
}

// ---------------------------------------------------------------------------
// amounts_for_position_liquidity_with_bound — table-driven
// ---------------------------------------------------------------------------

/// Helper: sqrt prices for the standard tick range [-10, 10] around tick 0.
fn sqrt_prices_for_range() -> (Uint256, Uint256, Uint256) {
    let sqrt_price = get_sqrt_ratio_at_tick(0).unwrap(); // price = 1.0
    let sqrt_lower = get_sqrt_ratio_at_tick(-10).unwrap();
    let sqrt_upper = get_sqrt_ratio_at_tick(10).unwrap();
    (sqrt_price, sqrt_lower, sqrt_upper)
}

#[test]
fn liquidity_bound_search_table() {
    let (sqrt_price, sqrt_lower, sqrt_upper) = sqrt_prices_for_range();

    struct Case {
        label: &'static str,
        liquidity: u128,
        max_0: u128,
        max_1: u128,
        /// If true, the target liquidity should fit without binary search.
        expect_exact_fit: bool,
    }

    let cases = vec![
        Case {
            label: "zero liquidity returns zero",
            liquidity: 0,
            max_0: 1_000_000,
            max_1: 1_000_000,
            expect_exact_fit: true,
        },
        Case {
            label: "generous bounds fit exactly",
            liquidity: 1_000,
            max_0: u128::MAX / 2,
            max_1: u128::MAX / 2,
            expect_exact_fit: true,
        },
        Case {
            label: "tight bounds force binary search reduction",
            liquidity: 1_000_000,
            max_0: 10,
            max_1: 10,
            expect_exact_fit: false,
        },
    ];

    for case in cases {
        let liq = Uint128::new(case.liquidity);
        let max_0 = Uint128::new(case.max_0);
        let max_1 = Uint128::new(case.max_1);

        let (fitted_liq, a0, a1) = amounts_for_position_liquidity_with_bound(
            sqrt_price, sqrt_lower, sqrt_upper, liq, max_0, max_1,
        )
        .unwrap_or_else(|e| panic!("FAIL [{}]: {e}", case.label));

        assert!(
            a0 <= max_0,
            "FAIL [{}]: a0 {a0} exceeds max {max_0}",
            case.label,
        );
        assert!(
            a1 <= max_1,
            "FAIL [{}]: a1 {a1} exceeds max {max_1}",
            case.label,
        );

        if case.expect_exact_fit {
            assert_eq!(fitted_liq, liq, "FAIL [{}]: expected exact fit", case.label,);
        } else {
            assert!(
                fitted_liq < liq,
                "FAIL [{}]: expected reduced liquidity, got {fitted_liq} >= {liq}",
                case.label,
            );
            assert!(
                fitted_liq > Uint128::zero(),
                "FAIL [{}]: fitted liquidity should be > 0",
                case.label,
            );
        }
    }
}
