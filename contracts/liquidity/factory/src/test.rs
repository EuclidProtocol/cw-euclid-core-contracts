#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::execute::{
        add_liquidity_request, execute_request_deregister_denom, execute_request_pool_creation,
        execute_swap_request,
    };
    use crate::query::{get_pool, pending_liquidity, pending_swaps, query_all_pools, query_state};
    use crate::state::{
        State, PENDING_LIQUIDITY, PENDING_SWAPS, STATE, TOKEN_TO_ESCROW, VLP_TO_POOL,
    };

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{
        attr, from_json, to_json_binary, Addr, Coin, CosmosMsg, DepsMut, IbcMsg, IbcTimeout,
        IbcTimeoutBlock, SubMsg, Uint128, WasmMsg,
    };
    use euclid::liquidity::LiquidityTxInfo;
    use euclid::msgs::factory::{AllPoolsResponse, GetPoolResponse, InstantiateMsg, StateResponse};
    use euclid::msgs::pool::{GetPendingLiquidityResponse, GetPendingSwapsResponse};
    use euclid::swap::{NextSwap, SwapInfo};
    use euclid::token::{PairInfo, Token, TokenInfo, TokenType};
    use euclid_ibc::msg::{ChainIbcExecuteMsg, ChainIbcSwapExecuteMsg};

    fn initialize_state(deps: &mut DepsMut) {
        let state = State {
            hub_channel: Some("hub_channel".to_string()),
            chain_id: "chain_id".to_string(),
            router_contract: "router_contract".to_string(),
            admin: "admin".to_string(),
            escrow_code_id: 1,
        };
        STATE.save(deps.storage, &state).unwrap();
    }
    #[test]
    fn test_query_state() {
        let mut deps = mock_dependencies();
        initialize_state(&mut deps.as_mut());

        let res = query_state(deps.as_ref()).unwrap();
        let value: StateResponse = from_json(&res).unwrap();

        assert_eq!(value.chain_id, "chain_id");
        assert_eq!(value.router_contract, "router_contract");
        assert_eq!(value.admin, "admin");
        assert_eq!(value.hub_channel, Some("hub_channel".to_string()));
    }
    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };

        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(
            res.attributes,
            vec![
                attr("method", "instantiate"),
                attr("router_contract", "router_contract"),
                attr("chain_id", "cosmos-testnet-14002"), // Adjust according to your mock environment
                attr("escrow_code_id", "1"),
            ]
        );
    }

    #[test]
    fn test_execute_request_pool_creation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);
        initialize_state(&mut deps.as_mut());

        let pair_info = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_1".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_1".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_2".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_2".to_string(),
                },
            },
        };

        let res = execute_request_pool_creation(
            deps.as_mut(),
            env.clone(),
            info,
            pair_info.clone(),
            Some(30),
        )
        .unwrap();
        assert_eq!(
            res.attributes,
            vec![attr("method", "request_pool_creation"),]
        );

        let ibc_msg = res
            .messages
            .iter()
            .find_map(|msg| {
                if let CosmosMsg::Ibc(IbcMsg::SendPacket {
                    channel_id, data, ..
                }) = &msg.msg
                {
                    Some((channel_id, data))
                } else {
                    None
                }
            })
            .expect("IBC message should be present");

        assert_eq!(ibc_msg.0, "hub_channel");

        let expected_msg = to_json_binary(&ChainIbcExecuteMsg::RequestPoolCreation {
            pool_rq_id: "creator-0".to_string(),
            pair_info,
        })
        .unwrap();

        assert_eq!(ibc_msg.1, &expected_msg);
    }

    #[test]
    fn test_execute_swap_request() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("sender", &[]);
        initialize_state(&mut deps.as_mut());

        // Define asset_in and asset_out
        let asset_in = TokenInfo {
            token: Token {
                id: "token_1".to_string(),
            },
            token_type: TokenType::Native {
                denom: "token_1".to_string(),
            },
        };

        let asset_out = TokenInfo {
            token: Token {
                id: "token_2".to_string(),
            },
            token_type: TokenType::Native {
                denom: "token_2".to_string(),
            },
        };

        let amount_in = Uint128::new(1000);
        let min_amount_out = Uint128::new(900);
        let swaps = vec![NextSwap {
            vlp_address: "vlp_address".to_string(),
        }];

        let vlp_address = "vlp_address".to_string(); // Define vlp_address as a String

        // Save TOKEN_TO_ESCROW with correct types
        TOKEN_TO_ESCROW
            .save(
                deps.as_mut().storage,
                Token {
                    id: "token_1".to_string(),
                },
                &Addr::unchecked("escrow".to_string()), // Ensure this key is correct
            )
            .unwrap();

        VLP_TO_POOL
            .save(
                deps.as_mut().storage,
                vlp_address.clone(),
                &PairInfo {
                    token_1: asset_in.clone(),
                    token_2: asset_out.clone(),
                },
            )
            .unwrap();

        // Call the execute_swap_request function
        let res = execute_swap_request(
            &mut deps.as_mut(),
            info,
            env.clone(),
            asset_in.clone(),
            asset_out.clone(),
            amount_in,
            min_amount_out,
            swaps.clone(),
            None,
            Some(30),
        )
        .unwrap();

        // Assert attributes of the response
        assert_eq!(
            res.attributes,
            vec![attr("method", "execute_request_swap"),]
        );

        // Assert IBC message details
        let ibc_msg = res
            .messages
            .iter()
            .find_map(|msg| {
                if let CosmosMsg::Ibc(IbcMsg::SendPacket {
                    channel_id, data, ..
                }) = &msg.msg
                {
                    Some((channel_id, data))
                } else {
                    None
                }
            })
            .expect("IBC message should be present");

        assert_eq!(ibc_msg.0, "hub_channel");

        let expected_msg = to_json_binary(&ChainIbcExecuteMsg::Swap(ChainIbcSwapExecuteMsg {
            to_address: "sender".to_string(),
            to_chain_id: "chain_id".to_string(), // Ensure to adjust according to your setup
            asset_in: asset_in.get_token(),
            amount_in,
            min_amount_out,
            swap_id: "0".to_string(), // Adjust if necessary based on your contract logic
            swaps,
        }))
        .unwrap();

        assert_eq!(ibc_msg.1, &expected_msg);
    }
    #[test]
    fn test_add_liquidity_request() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info(
            "sender",
            &[Coin::new(1000, "token_1"), Coin::new(1000, "token_2")],
        );
        initialize_state(&mut deps.as_mut());

        let vlp_address = "vlp_address".to_string();
        let token_1_liquidity = Uint128::new(500);
        let token_2_liquidity = Uint128::new(500);
        let slippage_tolerance = 50;

        let pair_info = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_1".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_1".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_2".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_2".to_string(),
                },
            },
        };

        VLP_TO_POOL
            .save(deps.as_mut().storage, vlp_address.clone(), &pair_info)
            .unwrap();

        let res = add_liquidity_request(
            deps.as_mut(),
            info,
            env.clone(),
            vlp_address.clone(),
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            None,
            Some(230), // Adjusted timeout value (example: 86400 seconds = 1 day)
        )
        .unwrap();

        assert_eq!(
            res.attributes,
            vec![attr("method", "add_liquidity_request"),]
        );

        let ibc_msg = res
            .messages
            .iter()
            .find_map(|msg| {
                if let CosmosMsg::Ibc(IbcMsg::SendPacket {
                    channel_id, data, ..
                }) = &msg.msg
                {
                    Some((channel_id, data))
                } else {
                    None
                }
            })
            .expect("IBC message should be present");

        assert_eq!(ibc_msg.0, "hub_channel");

        let expected_msg = to_json_binary(&ChainIbcExecuteMsg::AddLiquidity {
            token_1_liquidity,
            token_2_liquidity,
            slippage_tolerance,
            liquidity_id: "sender-0".to_string(),
            pool_address: "sender".to_string(),
            vlp_address,
        })
        .unwrap();

        assert_eq!(ibc_msg.1, &expected_msg);
    }

    #[test]
    fn test_execute_request_deregister_denom() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("sender", &[]);
        initialize_state(&mut deps.as_mut());

        let token = Token {
            id: "token_id".to_string(),
        };
        let denom = "denom1".to_string();

        TOKEN_TO_ESCROW
            .save(
                deps.as_mut().storage,
                token.clone().into(), // Ensure token is converted appropriately
                &Addr::unchecked("escrow_address".to_string()), // Use Addr::unchecked for testing
            )
            .unwrap();
        let res = execute_request_deregister_denom(
            deps.as_mut(),
            env,
            info,
            token.clone(),
            denom.clone(),
        )
        .unwrap();

        assert_eq!(
            res.attributes,
            vec![
                attr("method", "request_disallow_denom"),
                attr("token", token.id),
                attr("denom", denom.clone()),
            ]
        );

        let wasm_msg = res
            .messages
            .iter()
            .find_map(|msg| {
                if let SubMsg {
                    msg:
                        CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr, msg, ..
                        }),
                    ..
                } = &msg
                {
                    Some((contract_addr, msg))
                } else {
                    None
                }
            })
            .expect("Submessage with WASM message should be present");

        assert_eq!(wasm_msg.0, "escrow_address");

        let expected_msg = to_json_binary(&euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
            denom: denom.clone(),
        })
        .unwrap();

        assert_eq!(wasm_msg.1, &expected_msg);
    }

    #[test]
    fn test_get_pool() {
        let mut deps = mock_dependencies();
        initialize_state(&mut deps.as_mut());

        let vlp_address = "vlp_address".to_string();
        let pair_info = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_1".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_1".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_2".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_2".to_string(),
                },
            },
        };
        VLP_TO_POOL
            .save(deps.as_mut().storage, vlp_address.clone(), &pair_info)
            .unwrap();

        let res = get_pool(deps.as_ref(), vlp_address).unwrap();
        let value: GetPoolResponse = from_json(&res).unwrap();

        assert_eq!(value.pair_info, pair_info);
    }

    #[test]
    fn test_query_all_pools() {
        let mut deps = mock_dependencies();
        initialize_state(&mut deps.as_mut());

        let vlp_address_1 = "vlp_address_1".to_string();
        let pair_info_1 = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_1".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_1".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_2".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_2".to_string(),
                },
            },
        };

        let vlp_address_2 = "vlp_address_2".to_string();
        let pair_info_2 = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_3".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_3".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_4".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_4".to_string(),
                },
            },
        };

        VLP_TO_POOL
            .save(deps.as_mut().storage, vlp_address_1.clone(), &pair_info_1)
            .unwrap();
        VLP_TO_POOL
            .save(deps.as_mut().storage, vlp_address_2.clone(), &pair_info_2)
            .unwrap();

        let res = query_all_pools(deps.as_ref()).unwrap();
        let value: AllPoolsResponse = from_json(&res).unwrap();

        assert_eq!(value.pools.len(), 2);
        assert_eq!(value.pools[0].pair_info, pair_info_1);
        assert_eq!(value.pools[0].vlp, vlp_address_1);
        assert_eq!(value.pools[1].pair_info, pair_info_2);
        assert_eq!(value.pools[1].vlp, vlp_address_2);
    }
    #[test]
    fn test_pending_swaps() {
        let mut deps = mock_dependencies();
        initialize_state(&mut deps.as_mut());

        let user = "user".to_string();

        // Create example TokenInfo instances for the swaps
        let token_info_1 = TokenInfo {
            token: Token {
                id: "token_1".to_string(),
            },
            token_type: TokenType::Native {
                denom: "token_1".to_string(),
            },
        };

        let token_info_2 = TokenInfo {
            token: Token {
                id: "token_2".to_string(),
            },
            token_type: TokenType::Native {
                denom: "token_2".to_string(),
            },
        };

        // Create example SwapInfo instances
        let swap_1 = SwapInfo {
            asset_in: token_info_1.clone(),
            asset_out: token_info_2.clone(),
            amount_in: Uint128::new(100),
            min_amount_out: Uint128::new(90),
            swaps: vec![], // Add appropriate NextSwap instances if needed
            timeout: IbcTimeout::with_block(IbcTimeoutBlock {
                revision: 1,
                height: 123456,
            }),
            swap_id: "swap_id_1".to_string(),
        };

        let swap_2 = SwapInfo {
            asset_in: token_info_2.clone(),
            asset_out: token_info_1.clone(),
            amount_in: Uint128::new(200),
            min_amount_out: Uint128::new(180),
            swaps: vec![], // Add appropriate NextSwap instances if needed
            timeout: IbcTimeout::with_block(IbcTimeoutBlock {
                revision: 1,
                height: 123457,
            }),
            swap_id: "swap_id_2".to_string(),
        };

        PENDING_SWAPS
            .save(deps.as_mut().storage, (user.clone(), 0u128), &swap_1)
            .unwrap();
        PENDING_SWAPS
            .save(deps.as_mut().storage, (user.clone(), 1u128), &swap_2)
            .unwrap();

        let res = pending_swaps(deps.as_ref(), user.clone(), None, None).unwrap();
        let value: GetPendingSwapsResponse = from_json(&res).unwrap();

        assert_eq!(value.pending_swaps.len(), 2);
        assert_eq!(value.pending_swaps[0], swap_1);
        assert_eq!(value.pending_swaps[1], swap_2);
    }

    #[test]
    fn test_pending_liquidity() {
        let mut deps = mock_dependencies();
        initialize_state(&mut deps.as_mut());

        let user = "user".to_string();

        // Create example PairInfo instances for the liquidity transactions
        let pair_info_1 = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_1".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_1".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_2".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_2".to_string(),
                },
            },
        };

        let pair_info_2 = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_3".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_3".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_4".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_4".to_string(),
                },
            },
        };

        // Create example LiquidityTxInfo instances
        let liquidity_1 = LiquidityTxInfo {
            sender: user.clone(),
            token_1_liquidity: Uint128::new(1000),
            token_2_liquidity: Uint128::new(2000),
            liquidity_id: "liquidity_id_1".to_string(),
            vlp_address: "vlp_address_1".to_string(),
            pair_info: pair_info_1,
        };

        let liquidity_2 = LiquidityTxInfo {
            sender: user.clone(),
            token_1_liquidity: Uint128::new(3000),
            token_2_liquidity: Uint128::new(4000),
            liquidity_id: "liquidity_id_2".to_string(),
            vlp_address: "vlp_address_2".to_string(),
            pair_info: pair_info_2,
        };

        PENDING_LIQUIDITY
            .save(deps.as_mut().storage, (user.clone(), 0u128), &liquidity_1)
            .unwrap();
        PENDING_LIQUIDITY
            .save(deps.as_mut().storage, (user.clone(), 1u128), &liquidity_2)
            .unwrap();

        let res = pending_liquidity(deps.as_ref(), user.clone(), None, None).unwrap();
        let value: GetPendingLiquidityResponse = from_json(&res).unwrap();

        assert_eq!(value.pending_liquidity.len(), 2);
        assert_eq!(value.pending_liquidity[0], liquidity_1);
        assert_eq!(value.pending_liquidity[1], liquidity_2);
    }
}
