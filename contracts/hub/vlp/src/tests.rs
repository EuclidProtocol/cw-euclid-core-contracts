#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate};
    use crate::query::{calculate_lp_allocation_for_liquidity, query_simulate_swap};
    use crate::state::{State, BALANCES, CHAIN_LP_TOKENS, STATE};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_json, DepsMut, Response, Uint128};
    use euclid::chain::{ChainUid, CrossChainUser};
    use euclid::error::ContractError;
    use euclid::fee::{DenomFees, Fee, TotalFees};
    use euclid::msgs::vlp::{ExecuteMsg, GetSwapResponse, InstantiateMsg};
    use euclid::token::{Pair, Token};
    use std::collections::HashMap;

    fn init(deps: DepsMut) -> Response {
        let msg = InstantiateMsg {
            router: "router".to_string(),
            virtual_balance: "virtual_balance".to_string(),
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            fee: Fee {
                lp_fee_bps: 1,
                euclid_fee_bps: 1,
                recipient: CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            },
            execute: None,
            admin: "admin".to_string(),
        };
        let info = mock_info("router", &[]);
        instantiate(deps, mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(deps.as_mut());
        assert_eq!(0, res.messages.len());
        let expected_state = State {
            pair: Pair {
                token_1: Token::create("token1".to_string()).unwrap(),
                token_2: Token::create("token2".to_string()).unwrap(),
            },
            router: "router".to_string(),
            virtual_balance: "virtual_balance".to_string(),
            fee: Fee {
                lp_fee_bps: 1,
                euclid_fee_bps: 1,
                recipient: CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            },
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
            admin: "admin".to_string(),
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

        init(deps.as_mut());

        let sender = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "sender_address".to_string(),
        );

        let pair = Pair {
            token_1: Token::create("token1".to_string()).unwrap(),
            token_2: Token::create("token2".to_string()).unwrap(),
        };

        let msg = ExecuteMsg::RegisterPool {
            sender,
            pair,
            tx_id: "1".to_string(),
        };
        let info = mock_info("router", &coins(1000, "earth"));

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
        init(deps.as_mut());

        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(5),
            euclid_fee_bps: Some(4),
            recipient: Some(CrossChainUser::new(
                ChainUid::create("2".to_string()).unwrap(),
                "addr_2".to_string(),
            )),
        };
        let info = mock_info("not_admin", &[]);

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let info = mock_info("admin", &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let fee = STATE.load(&deps.storage).unwrap().fee;
        assert_eq!(
            fee,
            Fee {
                lp_fee_bps: 5,
                euclid_fee_bps: 4,
                recipient: CrossChainUser::new(
                    ChainUid::create("2".to_string()).unwrap(),
                    "addr_2".to_string(),
                ),
            }
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
    fn test_calculate_lp_allocation_for_liquidity() {
        let token_1_liquidity = Uint128::new(980);
        let token_2_liquidity = Uint128::new(1000);
        let total_reserve_1 = Uint128::new(10000);
        let total_reserve_2 = Uint128::new(10000);
        let total_lp_tokens = Uint128::new(10000); // 1:1 ratio for lp tokens to tokens present
        let slippage_tolerance_bps = 200; // 2% slippage tolerance

        // Call the function to test
        let lp_allocation = calculate_lp_allocation_for_liquidity(
            token_1_liquidity,
            token_2_liquidity,
            total_reserve_1,
            total_reserve_2,
            total_lp_tokens,
            slippage_tolerance_bps,
        )
        .unwrap();

        // Assert the expected LP allocation
        let expected_allocation = Uint128::new(980); // This value should be calculated based on the logic
        assert_eq!(lp_allocation, expected_allocation);
    }

    #[test]
    fn test_calculate_lp_allocation_for_liquidity_exceeded_slippage() {
        let token_1_liquidity = Uint128::new(100);
        let token_2_liquidity = Uint128::new(99);
        let total_reserve_1 = Uint128::new(100);
        let total_reserve_2 = Uint128::new(100);
        let total_lp_tokens = Uint128::new(100);
        let slippage_tolerance_bps = 0; // 0% slippage tolerance to force failure

        // Call the function to test and expect an error
        let err = calculate_lp_allocation_for_liquidity(
            token_1_liquidity,
            token_2_liquidity,
            total_reserve_1,
            total_reserve_2,
            total_lp_tokens,
            slippage_tolerance_bps,
        )
        .unwrap_err();

        match err {
            ContractError::LiquiditySlippageExceeded { .. } => (),
            _ => panic!("Expected slippage exceeded error, got {:?}", err),
        }
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
            router: "router".to_string(),
            virtual_balance: "virtual".to_string(),
            fee: Fee {
                lp_fee_bps: 30,
                euclid_fee_bps: 0,
                recipient: CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            },
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
            admin: "admin".to_string(),
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
        let response: GetSwapResponse = from_json(
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
}
