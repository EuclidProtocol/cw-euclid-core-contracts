#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::state::{Precisions, CONCENTRATED_BALANCES};
    use crate::testing::mock_querier::{mock_dependencies_custom, WasmMockQuerier};
    use crate::{
        contract::{execute, instantiate},
        state::{BALANCES, CHAIN_LP_TOKENS, STATE},
    };
    use cosmwasm_std::{
        coins,
        testing::{message_info, mock_env, MockQuerier},
        to_json_binary, Decimal, Decimal256, Response, Uint128, Uint64,
    };
    use cw_asset::{Asset, AssetBase, AssetInfo, AssetInfoBase};
    use euclid::{
        chain::{ChainUid, CrossChainUser},
        error::ContractError,
        fee::{DenomFees, Fee, TotalFees},
        msgs::concentrated_vlp::{ConcentratedPoolParams, ExecuteMsg, InstantiateMsg, PairType},
        pool::{stable_math::compute_stable_swap, State},
        token::{Pair, Token},
    };
    use std::collections::HashMap;

    fn init(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            WasmMockQuerier,
        >,
    ) -> Response {
        let router = deps.api.addr_make("router");
        let admin = deps.api.addr_make("admin");
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
        let recipient = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "addr".to_string(),
        );

        let fee = Fee::new(1, 2, recipient);

        let msg = InstantiateMsg {
            router: router.to_string(),
            virtual_balance: "virtual_balance".to_string(),
            fee: Fee::new(
                1,
                1,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            execute: None,
            admin: admin.to_string(),
            pair_type: PairType::Xyk {},
            asset_infos: vec![AssetInfo::native("1"), AssetInfo::native("2")],
            token_code_id: 4,
            factory_addr: deps.api.addr_make("factory").to_string(),
            init_params: Some(to_json_binary(&concentrated_vlp_params).unwrap()),
        };

        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies_custom(&[]);
        let res = init(&mut deps);

        // let router = deps.api.addr_make("router");
        // let admin = deps.api.addr_make("admin");
        // let expected_state = State {
        //     pair: Pair {
        //         token_1: Token::create("token1".to_string()).unwrap(),
        //         token_2: Token::create("token2".to_string()).unwrap(),
        //     },
        //     router: router.to_string(),
        //     virtual_balance: "virtual_balance".to_string(),
        //     fee: Fee::new(
        //         1,
        //         1,
        //         CrossChainUser::new(
        //             ChainUid::create("1".to_string()).unwrap(),
        //             "addr".to_string(),
        //         ),
        //     ),
        //     total_fees_collected: TotalFees {
        //         lp_fees: DenomFees {
        //             totals: HashMap::default(),
        //         },
        //         euclid_fees: DenomFees {
        //             totals: HashMap::default(),
        //         },
        //     },
        //     last_updated: 0,
        //     total_lp_tokens: Uint128::zero(),
        //     admin: admin.to_string(),
        // };
        // let state = STATE.load(&deps.storage).unwrap();
        // assert_eq!(state, expected_state);

        // let balance_1 = BALANCES.load(&deps.storage, state.pair.token_1).unwrap();
        // let expected_balance_1 = Uint128::zero();

        // assert_eq!(expected_balance_1, balance_1);

        // let balance_2 = BALANCES.load(&deps.storage, state.pair.token_2).unwrap();
        // let expected_balance_2 = Uint128::zero();

        // assert_eq!(balance_2, expected_balance_2);
    }

    #[test]
    fn test_execute_provide_liquidity() {
        let mut deps = mock_dependencies_custom(&[]);
        let env = mock_env();
        let factory_addr = deps.api.addr_make("factory");

        init(&mut deps);

        let assets = vec![
            Asset::native("1", Uint128::from(1000u128)),
            Asset::native("2", Uint128::from(1000u128)),
        ];
        let msg = ExecuteMsg::AddLiquidity {
            assets,
            slippage_tolerance: None,
            auto_stake: None,
            receiver: None,
            min_lp_to_receive: None,
        };

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &coins(1000, "earth"));

        // Store precissions, not sure when or where to do this in production
        Precisions::store_precisions(
            deps.as_mut(),
            &[AssetInfoBase::Native("1".to_string())],
            &factory_addr,
        )
        .unwrap();

        Precisions::store_precisions(
            deps.as_mut(),
            &[AssetInfoBase::Native("2".to_string())],
            &factory_addr,
        )
        .unwrap();

        let old_balances = CONCENTRATED_BALANCES
            .load(&deps.storage, &AssetInfoBase::Native("1".to_string()))
            .unwrap();

        println!("old balances: {}", old_balances);

        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        let new_balances = CONCENTRATED_BALANCES
            .load(&deps.storage, &AssetInfoBase::Native("1".to_string()))
            .unwrap();
        println!("new balances: {}", new_balances);
        assert_ne!(new_balances, old_balances);

        let sender = router.clone();
        let offer_asset = AssetBase::native("1", Uint128::new(100));

        let swap_msg = ExecuteMsg::Swap {
            sender,
            offer_asset,
            belief_price: None,
            max_spread: None,
            to: None,
        };

        let info = message_info(&router, &coins(100, "1"));
        let res = execute(deps.as_mut(), env.clone(), info, swap_msg).unwrap();
    }

    //     #[test]
    //     fn test_update_fee() {
    //         let mut deps = mock_dependencies();
    //         let env = mock_env();
    //         init(&mut deps);

    //         let msg = ExecuteMsg::UpdateFee {
    //             lp_fee_bps: Some(5),
    //             euclid_fee_bps: Some(4),
    //             recipient: Some(CrossChainUser::new(
    //                 ChainUid::create("2".to_string()).unwrap(),
    //                 "addr_2".to_string(),
    //             )),
    //         };
    //         let not_admin = deps.api.addr_make("not_admin");
    //         let info = message_info(&not_admin, &[]);

    //         let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    //         assert_eq!(err, ContractError::Unauthorized {});

    //         let admin = deps.api.addr_make("admin");
    //         let info = message_info(&admin, &[]);
    //         execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    //         let fee = STATE.load(&deps.storage).unwrap().fee;
    //         assert_eq!(
    //             fee,
    //             Fee::new(
    //                 5,
    //                 4,
    //                 CrossChainUser::new(
    //                     ChainUid::create("2".to_string()).unwrap(),
    //                     "addr_2".to_string(),
    //                 )
    //             )
    //         );

    //         // Exceed max bps
    //         let msg = ExecuteMsg::UpdateFee {
    //             lp_fee_bps: Some(5000),
    //             euclid_fee_bps: Some(4),
    //             recipient: Some(CrossChainUser::new(
    //                 ChainUid::create("2".to_string()).unwrap(),
    //                 "addr_2".to_string(),
    //             )),
    //         };

    //         let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
    //         assert_eq!(
    //             err,
    //             ContractError::new("LP Fee cannot exceed maximum limit")
    //         );

    //         let msg = ExecuteMsg::UpdateFee {
    //             lp_fee_bps: Some(50),
    //             euclid_fee_bps: Some(4000),
    //             recipient: Some(CrossChainUser::new(
    //                 ChainUid::create("2".to_string()).unwrap(),
    //                 "addr_2".to_string(),
    //             )),
    //         };

    //         let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
    //         assert_eq!(
    //             err,
    //             ContractError::new("Euclid Fee cannot exceed maximum limit")
    //         );
    //     }

    //     #[test]
    //     fn test_compute_swap_equal_pools() {
    //         // Test with equal pool sizes (1:1 ratio)
    //         let offer_asset = Decimal256::from_ratio(100u128, 1u128);
    //         let offer_pool = Decimal256::from_ratio(1000u128, 1u128);
    //         let ask_pool = Decimal256::from_ratio(1000u128, 1u128);
    //         println!("offer_asset in decimal: {:?}", offer_asset);
    //         let result =
    //             compute_stable_swap(&offer_asset, &offer_pool, &ask_pool, Uint64::new(1000)).unwrap();
    //         println!("result: {:?}", result);

    //         // For stable swap with equal pools, return amount should be very close to offer amount
    //         // with minimal spread
    //         assert_eq!(result.return_amount, Uint128::new(99)); // Allow for small rounding
    //         assert_eq!(result.spread_amount, Uint128::new(1));
    //     }

    //     #[test]
    //     fn test_compute_swap_imbalanced_pools() {
    //         // Test with imbalanced pools (2:1 ratio)
    //         let offer_asset = Decimal256::from_ratio(100u128, 1u128);
    //         let offer_pool = Decimal256::from_ratio(2000u128, 1u128);
    //         let ask_pool = Decimal256::from_ratio(1000u128, 1u128);

    //         let result =
    //             compute_stable_swap(&offer_asset, &offer_pool, &ask_pool, Uint64::new(100)).unwrap();

    //         // When pools are imbalanced, spread should be higher
    //         assert_eq!(result.return_amount, Uint128::new(67));
    //         assert_eq!(result.spread_amount, Uint128::new(33));
    //     }

    //     #[test]
    //     fn test_compute_swap_small_amount() {
    //         // Test with very small swap amount
    //         let offer_asset = Decimal256::from_ratio(1u128, 1u128);
    //         let offer_pool = Decimal256::from_ratio(1000000u128, 1u128);
    //         let ask_pool = Decimal256::from_ratio(1000000u128, 1u128);

    //         let result =
    //             compute_stable_swap(&offer_asset, &offer_pool, &ask_pool, Uint64::new(1000)).unwrap();

    //         // Small amounts should have minimal spread
    //         assert_eq!(result.return_amount, Uint128::new(1));
    //         assert_eq!(result.spread_amount, Uint128::new(0));
    //     }

    //     #[test]
    //     fn test_compute_swap_large_amount() {
    //         // Test with large swap amount relative to pool size
    //         let offer_asset = Decimal256::from_ratio(1000u128, 1u128);
    //         let offer_pool = Decimal256::from_ratio(2000u128, 1u128);
    //         let ask_pool = Decimal256::from_ratio(2000u128, 1u128);

    //         let result =
    //             compute_stable_swap(&offer_asset, &offer_pool, &ask_pool, Uint64::new(1000)).unwrap();

    //         // Large swaps should have higher spread due to impact on pool balance
    //         assert_eq!(result.return_amount, Uint128::new(946u128));
    //         assert_eq!(result.spread_amount, Uint128::new(54u128));
    //     }

    //     #[test]
    //     fn test_compute_swap_extreme_imbalance() {
    //         // Test with extremely imbalanced pools
    //         let offer_asset = Decimal256::from_ratio(100u128, 1u128);
    //         let offer_pool = Decimal256::from_ratio(10000u128, 1u128);
    //         let ask_pool = Decimal256::from_ratio(1000u128, 1u128);

    //         let result =
    //             compute_stable_swap(&offer_asset, &offer_pool, &ask_pool, Uint64::new(1000)).unwrap();

    //         // Highly imbalanced pools should result in higher spread
    //         assert_eq!(result.return_amount, Uint128::new(47u128));
    //         assert_eq!(result.spread_amount, Uint128::new(53u128));
    //     }

    //     #[test]
    //     fn test_compute_swap_large_values() {
    //         // Test with extremely imbalanced pools
    //         let offer_asset = Decimal256::from_ratio(1000000000000000000u128, 1u128);
    //         let offer_pool = Decimal256::from_ratio(1000000000000000000u128, 1u128);
    //         let ask_pool = Decimal256::from_ratio(1000000000000000000u128, 1u128);

    //         let result =
    //             compute_stable_swap(&offer_asset, &offer_pool, &ask_pool, Uint64::new(1000)).unwrap();

    //         // Highly imbalanced pools should result in higher spread
    //         assert_eq!(result.return_amount, Uint128::new(820871215252207999));
    //         assert_eq!(result.spread_amount, Uint128::new(179128784747792001));
    //     }
}
