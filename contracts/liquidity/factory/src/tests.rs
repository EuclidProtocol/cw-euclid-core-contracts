#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate};

    use crate::state::{State, HUB_CHANNEL, STATE};

    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{DepsMut, Response};
    use euclid::chain::ChainUid;
    use euclid::error::ContractError;
    use euclid::msgs::factory::{ExecuteMsg, InstantiateMsg};

    fn initialize_state(deps: &mut DepsMut) {
        let state = State {
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            router_contract: "router_contract".to_string(),
            admin: "admin".to_string(),
            escrow_code_id: 1,
            cw20_code_id: 2,
            is_native: true,
        };
        STATE.save(deps.storage, &state).unwrap();
    }

    fn init(deps: DepsMut) -> Response {
        let msg = InstantiateMsg {
            router_contract: "router".to_string(),
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            escrow_code_id: 1,
            cw20_code_id: 2,
            is_native: true,
        };
        let info = mock_info("owner", &[]);
        instantiate(deps, mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(deps.as_mut());
        assert_eq!(0, res.messages.len());
        let expected_state = State {
            router_contract: "router".to_string(),
            admin: "owner".to_string(),
            escrow_code_id: 1,
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            cw20_code_id: 2,
            is_native: true,
        };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
    }
    #[test]
    fn test_update_hub_channel() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("not_owner", &[]);
        init(deps.as_mut());

        HUB_CHANNEL
            .save(deps.as_mut().storage, &"1".to_string())
            .unwrap();
        let msg = ExecuteMsg::UpdateHubChannel {
            new_channel: "2".to_string(),
        };
        // Unauthorized
        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let info = mock_info("owner", &[]);
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        assert_eq!(HUB_CHANNEL.load(&deps.storage).unwrap(), "2".to_string());
    }

    //     #[test]
    //     fn test_execute_request_pool_creation() {
    //         let mut deps = mock_dependencies();
    //         let env = mock_env();
    //         let info = mock_info("creator", &[]);
    //         initialize_state(&mut deps.as_mut());

    //         let pair_info = PairInfo {
    //             token_1: TokenInfo {
    //                 token: Token {
    //                     id: "token_1".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_1".to_string(),
    //                 },
    //             },
    //             token_2: TokenInfo {
    //                 token: Token {
    //                     id: "token_2".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_2".to_string(),
    //                 },
    //             },
    //         };

    //         let res = execute_request_pool_creation(
    //             deps.as_mut(),
    //             env.clone(),
    //             info,
    //             pair_info.clone(),
    //             Some(30),
    //         )
    //         .unwrap();
    //         assert_eq!(
    //             res.attributes,
    //             vec![attr("method", "request_pool_creation"),]
    //         );

    //         let ibc_msg = res
    //             .messages
    //             .iter()
    //             .find_map(|msg| {
    //                 if let CosmosMsg::Ibc(IbcMsg::SendPacket {
    //                     channel_id, data, ..
    //                 }) = &msg.msg
    //                 {
    //                     Some((channel_id, data))
    //                 } else {
    //                     None
    //                 }
    //             })
    //             .expect("IBC message should be present");

    //         assert_eq!(ibc_msg.0, "hub_channel");

    //         let expected_msg = to_json_binary(&ChainIbcExecuteMsg::RequestPoolCreation {
    //             pool_rq_id: "creator-0".to_string(),
    //             pair_info,
    //         })
    //         .unwrap();

    //         assert_eq!(ibc_msg.1, &expected_msg);
    //     }

    //     #[test]
    //     fn test_add_liquidity_request() {
    //         let mut deps = mock_dependencies();
    //         let env = mock_env();
    //         let info = mock_info(
    //             "sender",
    //             &[Coin::new(1000, "token_1"), Coin::new(1000, "token_2")],
    //         );
    //         initialize_state(&mut deps.as_mut());

    //         let vlp_address = "vlp_address".to_string();
    //         let token_1_liquidity = Uint128::new(500);
    //         let token_2_liquidity = Uint128::new(500);
    //         let slippage_tolerance = 50;

    //         let pair_info = PairInfo {
    //             token_1: TokenInfo {
    //                 token: Token {
    //                     id: "token_1".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_1".to_string(),
    //                 },
    //             },
    //             token_2: TokenInfo {
    //                 token: Token {
    //                     id: "token_2".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_2".to_string(),
    //                 },
    //             },
    //         };

    //         VLP_TO_POOL
    //             .save(deps.as_mut().storage, vlp_address.clone(), &pair_info)
    //             .unwrap();

    //         let res = add_liquidity_request(
    //             deps.as_mut(),
    //             info,
    //             env.clone(),
    //             vlp_address.clone(),
    //             token_1_liquidity,
    //             token_2_liquidity,
    //             slippage_tolerance,
    //             None,
    //             Some(230), // Adjusted timeout value (example: 86400 seconds = 1 day)
    //         )
    //         .unwrap();

    //         assert_eq!(
    //             res.attributes,
    //             vec![attr("method", "add_liquidity_request"),]
    //         );

    //         let ibc_msg = res
    //             .messages
    //             .iter()
    //             .find_map(|msg| {
    //                 if let CosmosMsg::Ibc(IbcMsg::SendPacket {
    //                     channel_id, data, ..
    //                 }) = &msg.msg
    //                 {
    //                     Some((channel_id, data))
    //                 } else {
    //                     None
    //                 }
    //             })
    //             .expect("IBC message should be present");

    //         assert_eq!(ibc_msg.0, "hub_channel");

    //         let expected_msg = to_json_binary(&ChainIbcExecuteMsg::AddLiquidity {
    //             token_1_liquidity,
    //             token_2_liquidity,
    //             slippage_tolerance,
    //             liquidity_id: "sender-0".to_string(),
    //             pool_address: "sender".to_string(),
    //             vlp_address,
    //         })
    //         .unwrap();

    //         assert_eq!(ibc_msg.1, &expected_msg);
    //     }

    //     #[test]
    //     fn test_execute_request_deregister_denom() {
    //         let mut deps = mock_dependencies();
    //         let env = mock_env();
    //         let info = mock_info("sender", &[]);
    //         initialize_state(&mut deps.as_mut());

    //         let token = Token {
    //             id: "token_id".to_string(),
    //         };
    //         let denom = "denom1".to_string();

    //         TOKEN_TO_ESCROW
    //             .save(
    //                 deps.as_mut().storage,
    //                 token.clone().into(), // Ensure token is converted appropriately
    //                 &Addr::unchecked("escrow_address".to_string()), // Use Addr::unchecked for testing
    //             )
    //             .unwrap();
    //         let res = execute_request_deregister_denom(
    //             deps.as_mut(),
    //             env,
    //             info,
    //             token.clone(),
    //             denom.clone(),
    //         )
    //         .unwrap();

    //         assert_eq!(
    //             res.attributes,
    //             vec![
    //                 attr("method", "request_disallow_denom"),
    //                 attr("token", token.id),
    //                 attr("denom", denom.clone()),
    //             ]
    //         );

    //         let wasm_msg = res
    //             .messages
    //             .iter()
    //             .find_map(|msg| {
    //                 if let SubMsg {
    //                     msg:
    //                         CosmosMsg::Wasm(WasmMsg::Execute {
    //                             contract_addr, msg, ..
    //                         }),
    //                     ..
    //                 } = &msg
    //                 {
    //                     Some((contract_addr, msg))
    //                 } else {
    //                     None
    //                 }
    //             })
    //             .expect("Submessage with WASM message should be present");

    //         assert_eq!(wasm_msg.0, "escrow_address");

    //         let expected_msg = to_json_binary(&euclid::msgs::escrow::ExecuteMsg::DisallowDenom {
    //             denom: denom.clone(),
    //         })
    //         .unwrap();

    //         assert_eq!(wasm_msg.1, &expected_msg);
    //     }

    //     #[test]
    //     fn test_get_pool() {
    //         let mut deps = mock_dependencies();
    //         initialize_state(&mut deps.as_mut());

    //         let vlp_address = "vlp_address".to_string();
    //         let pair_info = PairInfo {
    //             token_1: TokenInfo {
    //                 token: Token {
    //                     id: "token_1".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_1".to_string(),
    //                 },
    //             },
    //             token_2: TokenInfo {
    //                 token: Token {
    //                     id: "token_2".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_2".to_string(),
    //                 },
    //             },
    //         };
    //         VLP_TO_POOL
    //             .save(deps.as_mut().storage, vlp_address.clone(), &pair_info)
    //             .unwrap();

    //         let res = get_pool(deps.as_ref(), vlp_address).unwrap();
    //         let value: GetPoolResponse = from_json(&res).unwrap();

    //         assert_eq!(value.pair_info, pair_info);
    //     }

    //     #[test]
    //     fn test_query_all_pools() {
    //         let mut deps = mock_dependencies();
    //         initialize_state(&mut deps.as_mut());

    //         let vlp_address_1 = "vlp_address_1".to_string();
    //         let pair_info_1 = PairInfo {
    //             token_1: TokenInfo {
    //                 token: Token {
    //                     id: "token_1".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_1".to_string(),
    //                 },
    //             },
    //             token_2: TokenInfo {
    //                 token: Token {
    //                     id: "token_2".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_2".to_string(),
    //                 },
    //             },
    //         };

    //         let vlp_address_2 = "vlp_address_2".to_string();
    //         let pair_info_2 = PairInfo {
    //             token_1: TokenInfo {
    //                 token: Token {
    //                     id: "token_3".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_3".to_string(),
    //                 },
    //             },
    //             token_2: TokenInfo {
    //                 token: Token {
    //                     id: "token_4".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_4".to_string(),
    //                 },
    //             },
    //         };

    //         VLP_TO_POOL
    //             .save(deps.as_mut().storage, vlp_address_1.clone(), &pair_info_1)
    //             .unwrap();
    //         VLP_TO_POOL
    //             .save(deps.as_mut().storage, vlp_address_2.clone(), &pair_info_2)
    //             .unwrap();

    //         let res = query_all_pools(deps.as_ref()).unwrap();
    //         let value: AllPoolsResponse = from_json(&res).unwrap();

    //         assert_eq!(value.pools.len(), 2);
    //         assert_eq!(value.pools[0].pair_info, pair_info_1);
    //         assert_eq!(value.pools[0].vlp, vlp_address_1);
    //         assert_eq!(value.pools[1].pair_info, pair_info_2);
    //         assert_eq!(value.pools[1].vlp, vlp_address_2);
    //     }
    //     #[test]
    //     fn test_pending_swaps() {
    //         let mut deps = mock_dependencies();
    //         initialize_state(&mut deps.as_mut());

    //         let user = "user".to_string();

    //         // Create example TokenInfo instances for the swaps
    //         let token_info_1 = TokenInfo {
    //             token: Token {
    //                 id: "token_1".to_string(),
    //             },
    //             token_type: TokenType::Native {
    //                 denom: "token_1".to_string(),
    //             },
    //         };

    //         let token_info_2 = TokenInfo {
    //             token: Token {
    //                 id: "token_2".to_string(),
    //             },
    //             token_type: TokenType::Native {
    //                 denom: "token_2".to_string(),
    //             },
    //         };

    //         // Create example SwapInfo instances
    //         let swap_1 = SwapInfo {
    //             asset_in: token_info_1.clone(),
    //             asset_out: token_info_2.clone(),
    //             amount_in: Uint128::new(100),
    //             min_amount_out: Uint128::new(90),
    //             swaps: vec![], // Add appropriate NextSwap instances if needed
    //             timeout: IbcTimeout::with_block(IbcTimeoutBlock {
    //                 revision: 1,
    //                 height: 123456,
    //             }),
    //             swap_id: "swap_id_1".to_string(),
    //         };

    //         let swap_2 = SwapInfo {
    //             asset_in: token_info_2.clone(),
    //             asset_out: token_info_1.clone(),
    //             amount_in: Uint128::new(200),
    //             min_amount_out: Uint128::new(180),
    //             swaps: vec![], // Add appropriate NextSwap instances if needed
    //             timeout: IbcTimeout::with_block(IbcTimeoutBlock {
    //                 revision: 1,
    //                 height: 123457,
    //             }),
    //             swap_id: "swap_id_2".to_string(),
    //         };

    //         PENDING_SWAPS
    //             .save(deps.as_mut().storage, (user.clone(), 0u128), &swap_1)
    //             .unwrap();
    //         PENDING_SWAPS
    //             .save(deps.as_mut().storage, (user.clone(), 1u128), &swap_2)
    //             .unwrap();

    //         let res = pending_swaps(deps.as_ref(), user.clone(), None, None).unwrap();
    //         let value: GetPendingSwapsResponse = from_json(&res).unwrap();

    //         assert_eq!(value.pending_swaps.len(), 2);
    //         assert_eq!(value.pending_swaps[0], swap_1);
    //         assert_eq!(value.pending_swaps[1], swap_2);
    //     }

    //     #[test]
    //     fn test_pending_liquidity() {
    //         let mut deps = mock_dependencies();
    //         initialize_state(&mut deps.as_mut());

    //         let user = "user".to_string();

    //         // Create example PairInfo instances for the liquidity transactions
    //         let pair_info_1 = PairInfo {
    //             token_1: TokenInfo {
    //                 token: Token {
    //                     id: "token_1".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_1".to_string(),
    //                 },
    //             },
    //             token_2: TokenInfo {
    //                 token: Token {
    //                     id: "token_2".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_2".to_string(),
    //                 },
    //             },
    //         };

    //         let pair_info_2 = PairInfo {
    //             token_1: TokenInfo {
    //                 token: Token {
    //                     id: "token_3".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_3".to_string(),
    //                 },
    //             },
    //             token_2: TokenInfo {
    //                 token: Token {
    //                     id: "token_4".to_string(),
    //                 },
    //                 token_type: TokenType::Native {
    //                     denom: "token_4".to_string(),
    //                 },
    //             },
    //         };

    //         // Create example LiquidityTxInfo instances
    //         let liquidity_1 = LiquidityTxInfo {
    //             sender: user.clone(),
    //             token_1_liquidity: Uint128::new(1000),
    //             token_2_liquidity: Uint128::new(2000),
    //             liquidity_id: "liquidity_id_1".to_string(),
    //             vlp_address: "vlp_address_1".to_string(),
    //             pair_info: pair_info_1,
    //         };

    //         let liquidity_2 = LiquidityTxInfo {
    //             sender: user.clone(),
    //             token_1_liquidity: Uint128::new(3000),
    //             token_2_liquidity: Uint128::new(4000),
    //             liquidity_id: "liquidity_id_2".to_string(),
    //             vlp_address: "vlp_address_2".to_string(),
    //             pair_info: pair_info_2,
    //         };

    //         PENDING_LIQUIDITY
    //             .save(deps.as_mut().storage, (user.clone(), 0u128), &liquidity_1)
    //             .unwrap();
    //         PENDING_LIQUIDITY
    //             .save(deps.as_mut().storage, (user.clone(), 1u128), &liquidity_2)
    //             .unwrap();

    //         let res = pending_liquidity(deps.as_ref(), user.clone(), None, None).unwrap();
    //         let value: GetPendingLiquidityResponse = from_json(&res).unwrap();

    //         assert_eq!(value.pending_liquidity.len(), 2);
    //         assert_eq!(value.pending_liquidity[0], liquidity_1);
    //         assert_eq!(value.pending_liquidity[1], liquidity_2);
    //     }
}
