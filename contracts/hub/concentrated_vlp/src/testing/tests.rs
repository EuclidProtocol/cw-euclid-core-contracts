#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, reply};
    use crate::state::{Precisions, CONCENTRATED_BALANCES};
    use crate::testing::mock_querier::{mock_dependencies_custom, WasmMockQuerier};
    use cosmwasm_std::{
        coins,
        testing::{message_info, mock_env},
        to_json_binary, Decimal, Response, Uint128,
    };
    use cosmwasm_std::{Binary, Env, Reply, SubMsgResponse, SubMsgResult};
    use cw_asset::{Asset, AssetBase, AssetInfo, AssetInfoBase};
    use euclid::msgs::concentrated_vlp::MsgCreateDenomResponse;
    use euclid::{
        chain::{ChainUid, CrossChainUser},
        fee::Fee,
        msgs::concentrated_vlp::{ConcentratedPoolParams, ExecuteMsg, InstantiateMsg, PairType},
    };

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
        // let recipient = CrossChainUser::new(
        //     ChainUid::create("1".to_string()).unwrap(),
        //     "addr".to_string(),
        // );

        // let fee = Fee::new(1, 2, recipient);

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
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        reply(
            deps.as_mut(),
            mock_env(),
            Reply {
                id: 1,
                result: SubMsgResult::Ok(SubMsgResponse {
                    #[allow(deprecated)]
                    data: Some(
                        MsgCreateDenomResponse {
                            new_token_denom: "new_token_denom".to_string(),
                        }
                        .to_proto_bytes()
                        .into(),
                    ),
                    msg_responses: vec![],
                    events: vec![],
                }),
                payload: Binary::default(),
                gas_used: 20,
            },
        )
        .unwrap();
        res
    }

    fn add_liquidity(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            WasmMockQuerier,
        >,
        env: Env,
        assets: Vec<Asset>,
        slippage_tolerance: Option<Decimal>,
        auto_stake: Option<bool>,
        receiver: Option<String>,
        min_lp_to_receive: Option<Uint128>,
    ) -> Response {
        init(deps);

        let msg = ExecuteMsg::AddLiquidity {
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
        };
        let factory_addr = deps.api.addr_make("factory");
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

        execute(deps.as_mut(), env.clone(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies_custom(&[]);
        init(&mut deps);
    }

    #[test]
    fn test_add_liquidity() {
        let mut deps = mock_dependencies_custom(&[]);
        let env = mock_env();
        init(&mut deps);

        let old_balances = CONCENTRATED_BALANCES
            .load(&deps.storage, &AssetInfoBase::Native("1".to_string()))
            .unwrap();

        let assets = vec![
            Asset::native("1", Uint128::from(1000u128)),
            Asset::native("2", Uint128::from(1000u128)),
        ];

        add_liquidity(&mut deps, env, assets, None, None, None, None);

        let new_balances = CONCENTRATED_BALANCES
            .load(&deps.storage, &AssetInfoBase::Native("1".to_string()))
            .unwrap();
        println!("new balances: {}", new_balances);
        assert_ne!(new_balances, old_balances);
    }

    #[test]
    fn test_withdraw_liquidity() {
        let mut deps = mock_dependencies_custom(&[]);
        let env = mock_env();
        let router = deps.api.addr_make("router");
        init(&mut deps);

        let old_balances = CONCENTRATED_BALANCES
            .load(&deps.storage, &AssetInfoBase::Native("1".to_string()))
            .unwrap();

        let assets = vec![
            Asset::native("1", Uint128::from(1000u128)),
            Asset::native("2", Uint128::from(1000u128)),
        ];

        add_liquidity(&mut deps, env.clone(), assets, None, None, None, None);

        let new_balances = CONCENTRATED_BALANCES
            .load(&deps.storage, &AssetInfoBase::Native("1".to_string()))
            .unwrap();
        println!("new balances: {}", new_balances);
        assert_ne!(new_balances, old_balances);

        let msg = ExecuteMsg::RemoveLiquidity { assets: vec![] };
        let info = message_info(&router, &coins(1000, "new_token_denom"));
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        println!("res {:?}", res);
        let new_balances = CONCENTRATED_BALANCES
            .load(&deps.storage, &AssetInfoBase::Native("1".to_string()))
            .unwrap();
        println!("new balances: {}", new_balances);
        assert_eq!(new_balances.u128(), 999001);
    }

    #[test]
    fn test_execute_swap() {
        let mut deps = mock_dependencies_custom(&[]);
        let env = mock_env();

        let router = deps.api.addr_make("router");

        init(&mut deps);

        let assets = vec![
            Asset::native("1", Uint128::from(1000u128)),
            Asset::native("2", Uint128::from(1000u128)),
        ];

        add_liquidity(&mut deps, env.clone(), assets, None, None, None, None);

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
        println!("res {:?}", res);
    }
}
