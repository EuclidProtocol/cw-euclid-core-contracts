#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate};
    use crate::query::query_simulate_swap;
    use crate::state::{BALANCES, CHAIN_LP_TOKENS, STATE};
    use cosmwasm_std::testing::{MockQuerier, message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{Addr, Response, Uint128, coins, from_json};
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::error::ContractError;
    use euclid::fee::{DenomFees, Fee, TotalFees};
    use euclid::msgs::vlp::base::{
        GetSwapQueryResponse, State, VlpAddLiquidityMsg, VlpRegisterPoolMsg, VlpRemoveLiquidityMsg,
        VlpSwapMsg,
    };
    use euclid::msgs::vlp::cp::msg::{ExecuteMsg, InstantiateMsg};
    use euclid::token::{Pair, PairWithAmount, Token};
    use std::collections::HashMap;

    fn init(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            MockQuerier,
        >,
    ) -> Response {
        let admin = deps.api.addr_make("admin");

        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            fee: Fee::new(
                1,
                1,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            execute: None,
            admin,
        };
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let res = init(&mut deps);
        assert_eq!(0, res.messages.len());
        let admin = deps.api.addr_make("admin");
        let expected_state = State {
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            router,
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            fee: Fee::new(
                1,
                1,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: 0,
            total_lp_tokens: Uint128::zero(),
            paused: false,
            admin,
        };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);

        let balance_1 = BALANCES.load(&deps.storage, state.pair.token_1).unwrap();
        let expected_balance_1 = Uint128::zero();

        assert_eq!(expected_balance_1, balance_1);

        let balance_2 = BALANCES.load(&deps.storage, state.pair.token_2).unwrap();
        let expected_balance_2 = Uint128::zero();

        assert_eq!(balance_2, expected_balance_2);
    }

    #[test]
    fn test_execute_register_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        init(&mut deps);

        let sender = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "sender_address".to_string(),
        );

        let pair = Pair {
            token_1: Token::create("token1".to_string()).unwrap(),
            token_2: Token::create("token2".to_string()).unwrap(),
        };

        let msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender,
            pair,
            tx_id: "1".to_string(),
        });
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &coins(1000, "earth"));

        // Execute the register_pool function
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 0); // Ensure no extra messages are sent

        let state = CHAIN_LP_TOKENS
            .load(&deps.storage, ChainUid::create("1".to_string()).unwrap())
            .unwrap();
        assert_eq!(state, Uint128::zero())
    }

    #[test]
    fn test_update_fee() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5),
            euclid_fee_bps: Some(4),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let fee = STATE.load(&deps.storage).unwrap().fee;
        assert_eq!(
            fee,
            Fee::new(
                5,
                4,
                CrossChainUser::new(
                    ChainUid::create("2".to_string()).unwrap(),
                    "addr_2".to_string(),
                )
            )
        );

        // Exceed max bps
        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5000),
            euclid_fee_bps: Some(4),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("LP Fee cannot exceed maximum limit")
        );

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(50),
            euclid_fee_bps: Some(4000),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(
            err,
            ContractError::new("Euclid Fee cannot exceed maximum limit")
        );
    }

    #[test]
    fn test_paused_blocks_actions() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        init(&mut deps);

        let router = deps.api.addr_make("router");
        let admin = deps.api.addr_make("admin");

        let pair = Pair {
            token_1: Token::create("token1".to_string()).unwrap(),
            token_2: Token::create("token2".to_string()).unwrap(),
        };

        let sender = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "sender_address".to_string(),
        );

        // Set up initial pool and liquidity while unpaused
        let register_msg = ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
            sender: sender.clone(),
            pair: pair.clone(),
            tx_id: "register_tx".to_string(),
        });

        let register_info = message_info(&router, &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            register_info.clone(),
            register_msg,
        )
        .unwrap();

        let liquidity = PairWithAmount::new(
            pair.token_1.with_amount(Uint128::new(100)),
            pair.token_2.with_amount(Uint128::new(100)),
        )
        .unwrap();

        let lp_allocation = CHAIN_LP_TOKENS
            .load(&deps.storage, sender.chain_uid.clone())
            .unwrap();

        // Pause the contract
        let pause_msg = ExecuteMsg::UpdateState {
            admin: None,
            paused: Some(true),
        };
        let admin_info = message_info(&admin, &[]);
        execute(deps.as_mut(), env.clone(), admin_info.clone(), pause_msg).unwrap();

        // Actions should be blocked when paused
        let err = execute(
            deps.as_mut(),
            env.clone(),
            register_info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add_paused".to_string(),
                liquidity: liquidity.clone(),
                slippage_tolerance_bps: 10,
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ContractPaused {});

        let err = execute(
            deps.as_mut(),
            env.clone(),
            register_info.clone(),
            ExecuteMsg::RemoveLiquidity(VlpRemoveLiquidityMsg {
                sender: sender.clone(),
                lp_allocation,
                tx_id: "remove_paused".to_string(),
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ContractPaused {});

        let err = execute(
            deps.as_mut(),
            env.clone(),
            register_info.clone(),
            ExecuteMsg::Swap(VlpSwapMsg {
                sender: sender.clone(),
                tx_id: "swap_paused".to_string(),
                asset_in: pair.token_1.clone(),
                amount_in: Uint128::new(10),
                min_token_out: Uint128::new(1),
                next_swaps: vec![],
                test_fail: None,
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ContractPaused {});

        // UpdateFee is not affected by pause
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info.clone(),
            ExecuteMsg::UpdateFee {
                lp_fee_bps: Some(2),
                euclid_fee_bps: Some(1),
                recipient: Some(sender.clone()),
            },
        )
        .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            register_info,
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: CrossChainUser::new(
                    ChainUid::create("2".to_string()).unwrap(),
                    "second_sender".to_string(),
                ),
                pair: pair.clone(),
                tx_id: "register_paused".to_string(),
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ContractPaused {});

        // UpdateState should remain allowed even when paused
        let unpause_msg = ExecuteMsg::UpdateState {
            admin: None,
            paused: Some(false),
        };
        execute(deps.as_mut(), env, admin_info, unpause_msg).unwrap();
    }

    #[test]
    fn test_simulate_swap_with_spread() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Setup test state
        let pair = Pair {
            token_1: Token::create("uatom".to_string()).unwrap(),
            token_2: Token::create("uosmo".to_string()).unwrap(),
        };

        let state = State {
            pair: pair.clone(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            fee: Fee::new(
                30,
                0,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: env.block.time.seconds(),
            total_lp_tokens: Uint128::new(1000),
            paused: false,
            admin: Addr::unchecked("admin"),
        };

        STATE.save(deps.as_mut().storage, &state).unwrap();

        // Setup reserves with imbalanced ratio to create spread
        let reserve_1 = Uint128::new(1000);
        let reserve_2 = Uint128::new(500);

        BALANCES
            .save(deps.as_mut().storage, pair.token_1.clone(), &reserve_1)
            .unwrap();
        BALANCES
            .save(deps.as_mut().storage, pair.token_2.clone(), &reserve_2)
            .unwrap();

        // Simulate swap
        let swap_amount = Uint128::new(100);
        let response: GetSwapQueryResponse = from_json(
            query_simulate_swap(deps.as_ref(), pair.token_1, swap_amount, vec![]).unwrap(),
        )
        .unwrap();

        // Expected spread calculation:
        // Initial price ratio = 1000/500 = 2
        // Actual received = calculate_swap(97, 1000, 500) ≈ 46
        // Ideal received = 100 * (500/1000) = 50
        // Spread ≈ 50 - 46 = 4
        assert_eq!(response.asset_out, pair.token_2);
        assert_eq!(
            response.amount_out,
            Uint128::new(46),
            "Amount out is not correct"
        );
        assert_eq!(
            response.spread_amount,
            Uint128::new(4),
            "Spread amount is not correct"
        );
    }

    #[test]
    fn test_swap_with_large_reserve_ratio() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Setup test state
        let pair = Pair {
            token_1: Token::create("uatom".to_string()).unwrap(),
            token_2: Token::create("uosmo".to_string()).unwrap(),
        };

        let state = State {
            pair: pair.clone(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            fee: Fee::new(
                30,
                0,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: env.block.time.seconds(),
            total_lp_tokens: Uint128::new(1000),
            paused: false,
            admin: Addr::unchecked("admin"),
        };

        STATE.save(deps.as_mut().storage, &state).unwrap();

        // Setup reserves with imbalanced ratio to create spread
        let reserve_1 = Uint128::new(9971294131355738400);
        let reserve_2 = Uint128::new(64769345018139098454);

        BALANCES
            .save(deps.as_mut().storage, pair.token_1.clone(), &reserve_1)
            .unwrap();
        BALANCES
            .save(deps.as_mut().storage, pair.token_2.clone(), &reserve_2)
            .unwrap();

        // Simulate swap
        let swap_amount = Uint128::new(10000000000000000);
        let response: GetSwapQueryResponse = from_json(
            query_simulate_swap(deps.as_ref(), pair.token_1, swap_amount, vec![]).unwrap(),
        )
        .unwrap();

        // Expected spread calculation:
        // Initial price ratio = 1000/500 = 2
        // Actual received = calculate_swap(97, 1000, 500) ≈ 46
        // Ideal received = 100 * (500/1000) = 50
        // Spread ≈ 50 - 46 = 4
        assert_eq!(response.asset_out, pair.token_2);
        assert_eq!(
            response.amount_out,
            Uint128::new(64696251029190591),
            "Amount out is not correct"
        );
        assert_eq!(
            response.spread_amount,
            Uint128::new(64687854381176),
            "Spread amount is not correct"
        );
    }
}
