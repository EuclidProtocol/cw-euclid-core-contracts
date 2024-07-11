#[cfg(test)]
mod tests {
    use std::borrow::Borrow;

    use crate::contract::{execute, instantiate};
    use crate::query::{self, query_all_chains, query_all_vlps, query_chain, query_vlp};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{
        from_json, to_json_binary, Binary, ContractResult, CosmosMsg, IbcMsg, SystemError,
        SystemResult, Uint128,
    };
    use euclid::chain::ChainUid;
    use euclid::error::ContractError;
    use euclid::msgs::router::{
        AllChainResponse, AllVlpResponse, Chain, ChainResponse, ExecuteMsg, InstantiateMsg,
        QuerySimulateSwap, SimulateSwapResponse, VlpResponse,
    };
    use euclid::msgs::vlp::GetSwapResponse;
    use euclid::swap::NextSwapPair;
    use euclid::token::Token;
    use euclid_ibc::msg::HubIbcExecuteMsg;
    // use euclid_ibc::msg::{InstantiateMsg, ExecuteMsg, QueryMsg};
    use crate::state::{State, ESCROW_BALANCES, STATE, VLPS};

    struct TestToken {
        name: &'static str,
        token: Token,
        expected_error: Option<ContractError>,
    }

    struct TestInstantiateMsg {
        name: &'static str,
        msg: InstantiateMsg,
        expected_error: Option<ContractError>,
    }

    struct TestExecuteMsg {
        name: &'static str,
        msg: ExecuteMsg,
        expected_error: Option<ContractError>,
    }
    struct TestQueryState {
        name: &'static str,
        expected_admin: String,
        expected_vlp_code_id: u64,
        expected_vcoin_address: Option<String>,
        expected_error: Option<ContractError>,
    }

    struct TestQueryAllVlps {
        name: &'static str,
        vlps: Vec<((Token, Token), String)>,
        expected_response: AllVlpResponse,
        expected_error: Option<ContractError>,
    }

    struct TestQueryVlp {
        name: &'static str,
        token_1: Token,
        token_2: Token,
        expected_response: VlpResponse,
        expected_error: Option<ContractError>,
    }

    struct TestQueryAllChains {
        name: &'static str,
        chains: Vec<(String, String)>,
        expected_response: AllChainResponse,
        expected_error: Option<ContractError>,
    }

    struct TestQueryChain {
        name: &'static str,
        chain_id: String,
        expected_response: ChainResponse,
        expected_error: Option<ContractError>,
    }

    struct TestQuerySimulateSwap {
        name: &'static str,
        msg: QuerySimulateSwap,
        expected_response: SimulateSwapResponse,
        expected_error: Option<ContractError>,
    }
    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let test_cases = vec![TestInstantiateMsg {
            name: "Valid instantiate message",
            msg: InstantiateMsg {
                vlp_code_id: 1,
                vcoin_code_id: 2,
            },
            expected_error: None,
        }];

        for test in test_cases {
            let res = instantiate(deps.as_mut(), env.clone(), info.clone(), test.msg.clone());
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => assert!(res.is_ok(), "{}", test.name),
            }
        }
    }
    #[test]
    fn test_execute_update_vlp_code_id() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        // Instantiate the contract first
        let msg = InstantiateMsg {
            vlp_code_id: 1,
            vcoin_code_id: 2,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let test_cases = vec![
            TestExecuteMsg {
                name: "Update VLP Code ID by admin",
                msg: ExecuteMsg::UpdateVLPCodeId { new_vlp_code_id: 2 },
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Update VLP Code ID by non-admin",
                msg: ExecuteMsg::UpdateVLPCodeId { new_vlp_code_id: 3 },
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];

        for test in test_cases {
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-admin") {
                    mock_info("non-admin", &[])
                } else {
                    info.clone()
                },
                test.msg.clone(),
            );
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => {
                    assert!(res.is_ok(), "{}", test.name);

                    // Verify the state was updated
                    let state: State = STATE.load(&deps.storage).unwrap();
                    if let ExecuteMsg::UpdateVLPCodeId { new_vlp_code_id } = test.msg {
                        assert_eq!(state.vlp_code_id, new_vlp_code_id);
                    }
                }
            }
        }
    }

    #[test]
    fn test_execute_register_factory() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        // Instantiate the contract first
        let msg = InstantiateMsg {
            vlp_code_id: 1,
            vcoin_code_id: 2,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let test_cases = vec![
            TestExecuteMsg {
                name: "Register factory by admin",
                msg: ExecuteMsg::RegisterFactory {
                    channel: "channel-1".to_string(),
                    timeout: Some(60),
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                    tx_id: "1".to_string(),
                },
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Register factory by non-admin",
                msg: ExecuteMsg::RegisterFactory {
                    channel: "channel-1".to_string(),
                    timeout: Some(60),
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                    tx_id: "1".to_string(),
                },
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];

        for test in test_cases {
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-admin") {
                    mock_info("non-admin", &[])
                } else {
                    info.clone()
                },
                test.msg.clone(),
            );
            match test.expected_error {
                Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
                None => {
                    assert!(res.is_ok(), "{}", test.name);

                    // Verify the response
                    let res = res.unwrap();
                    assert_eq!(res.attributes.len(), 3);
                    assert_eq!(res.attributes[0].key, "method");
                    assert_eq!(res.attributes[0].value, "register_factory");
                    assert_eq!(res.attributes[1].key, "channel");
                    assert_eq!(res.attributes[1].value, "channel-1");
                    assert_eq!(res.attributes[2].key, "timeout");
                    assert_eq!(res.attributes[2].value, "60");

                    // Verify the IBC packet message
                    let messages = res.messages;
                    assert_eq!(messages.len(), 1);
                    if let CosmosMsg::Ibc(IbcMsg::SendPacket {
                        channel_id,
                        timeout,
                        data,
                    }) = &messages[0].msg
                    {
                        assert_eq!(channel_id, "channel-1");
                        assert!(timeout.timestamp().is_some());
                        let msg: HubIbcExecuteMsg = from_json(data).unwrap();
                        assert_eq!(
                            msg,
                            HubIbcExecuteMsg::RegisterFactory {
                                chain_uid: ChainUid::create("1".to_string()).unwrap(),
                                tx_id: "1".to_string(),
                            }
                        );
                    } else {
                        panic!("Expected IbcMsg::SendPacket");
                    }
                }
            }
        }
    }
    // #[test]
    // fn test_query_all_vlps() {
    //     let mut deps = mock_dependencies();

    //     // Test cases
    //     let test_cases = vec![TestQueryAllVlps {
    //         name: "Valid query all vlps",
    //         vlps: vec![
    //             (
    //                 (
    //                     Token::create("token_1".to_string()).unwrap(),
    //                     Token::create("token_2".to_string()).unwrap(),
    //                 ),
    //                 "vlp1".to_string(),
    //             ),
    //             (
    //                 (
    //                     Token::create("token_3".to_string()).unwrap(),
    //                     Token::create("token_4".to_string()).unwrap(),
    //                 ),
    //                 "vlp2".to_string(),
    //             ),
    //         ],
    //         expected_response: AllVlpResponse {
    //             vlps: vec![
    //                 VlpResponse {
    //                     vlp: "vlp1".to_string(),
    //                     token_1: Token::create("token_1".to_string()).unwrap(),
    //                     token_2: Token::create("token_2".to_string()).unwrap(),
    //                 },
    //                 VlpResponse {
    //                     vlp: "vlp2".to_string(),
    //                     token_1: Token::create("token_3".to_string()).unwrap(),
    //                     token_2: Token::create("token_4".to_string()).unwrap(),
    //                 },
    //             ],
    //         },
    //         expected_error: None,
    //     }];

    //     for mut test in test_cases {
    //         for ((token1, token2), vlp) in &test.vlps {
    //             let vlp_response = VlpResponse {
    //                 vlp: vlp.clone(),
    //                 token_1: token1.clone(),
    //                 token_2: token2.clone(),
    //             };
    //             VLPS.save(&mut deps.storage, (token1.clone(), token2.clone()), vlp)
    //                 .unwrap();
    //         }
    //         let res = query_all_vlps(deps.as_ref());
    //         match test.expected_error {
    //             Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
    //             None => {
    //                 let bin = res.unwrap();
    //                 let mut response: AllVlpResponse = from_json(&bin).unwrap();

    //                 // Sort both actual and expected responses to ensure consistent order
    //                 response.vlps.sort_by(|a, b| a.vlp.cmp(&b.vlp));
    //                 test.expected_response
    //                     .vlps
    //                     .sort_by(|a, b| a.vlp.cmp(&b.vlp));

    //                 assert_eq!(response, test.expected_response, "{}", test.name);
    //             }
    //         }
    //     }
    // }

    // #[test]
    // fn test_query_vlp() {
    //     let mut deps = mock_dependencies();

    //     // Test cases
    //     let test_cases = vec![TestQueryVlp {
    //         name: "Valid query vlp",
    //         token_1: Token::create("token_1".to_string()).unwrap(),
    //         token_2: Token::create("token_2".to_string()).unwrap(),
    //         expected_response: VlpResponse {
    //             vlp: "vlp1".to_string(),
    //             token_1: Token::create("token_1".to_string()).unwrap(),
    //             token_2: Token::create("token_2".to_string()).unwrap(),
    //         },
    //         expected_error: None,
    //     }];

    //     for test in test_cases {
    //         VLPS.save(
    //             &mut deps.storage,
    //             (test.token_1.clone(), test.token_2.clone()),
    //             &test.expected_response.vlp,
    //         )
    //         .unwrap();
    //         let res = query_vlp(deps.as_ref(), test.token_1.clone(), test.token_2.clone());
    //         match test.expected_error {
    //             Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
    //             None => {
    //                 let bin = res.unwrap();
    //                 let response: VlpResponse = from_json(&bin).unwrap();
    //                 assert_eq!(response, test.expected_response);
    //             }
    //         }
    //     }
    // }
    // #[test]
    // fn test_query_all_chains() {
    //     let mut deps = mock_dependencies();

    //     // Test cases
    //     let test_cases = vec![TestQueryAllChains {
    //         name: "Valid query all chains",
    //         chains: vec![
    //             ("chain1".to_string(), "factory_chain_id_1".to_string()),
    //             ("chain2".to_string(), "factory_chain_id_2".to_string()),
    //         ],
    //         expected_response: AllChainResponse {
    //             chains: vec![
    //                 ChainResponse {
    //                     chain: Chain {
    //                         factory_chain_id: "factory_chain_id_1".to_string(),
    //                         factory: "factory_1".to_string(),
    //                         from_hub_channel: "hub_channel_1".to_string(),
    //                         from_factory_channel: "factory_channel_1".to_string(),
    //                     },
    //                     chain_uid: ChainUid::create("1".to_string()).unwrap(),
    //                 },
    //                 ChainResponse {
    //                     chain: Chain {
    //                         factory_chain_id: "factory_chain_id_2".to_string(),
    //                         factory: "factory_2".to_string(),
    //                         from_hub_channel: "hub_channel_2".to_string(),
    //                         from_factory_channel: "factory_channel_2".to_string(),
    //                     },
    //                     chain_uid: ChainUid::create("2".to_string()).unwrap(),
    //                 },
    //             ],
    //         },
    //         expected_error: None,
    //     }];

    //     for mut test in test_cases {
    //         for (chain_id, factory_chain_id) in &test.chains {
    //             // Create Chain object from chain_data string
    //             let chain = Chain {
    //                 factory_chain_id: factory_chain_id.clone(),
    //                 factory: format!("factory_{}", chain_id.chars().last().unwrap()), // Example factory name
    //                 from_hub_channel: format!("hub_channel_{}", chain_id.chars().last().unwrap()), // Example hub channel name
    //                 from_factory_channel: format!(
    //                     "factory_channel_{}",
    //                     chain_id.chars().last().unwrap()
    //                 ), // Example factory channel name
    //             };

    //             // Save the Chain object into storage
    //             CHAIN_ID_TO_CHAIN
    //                 .save(&mut deps.storage, chain_id.clone(), &chain)
    //                 .unwrap();
    //         }

    //         let res = query_all_chains(deps.as_ref());
    //         match test.expected_error {
    //             Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
    //             None => {
    //                 let bin = res.unwrap();
    //                 let mut response: AllChainResponse = from_json(&bin).unwrap();

    //                 // Sort both actual and expected responses to ensure consistent order
    //                 response
    //                     .chains
    //                     .sort_by(|a, b| a.chain.factory_chain_id.cmp(&b.chain.factory_chain_id));
    //                 test.expected_response
    //                     .chains
    //                     .sort_by(|a, b| a.chain.factory_chain_id.cmp(&b.chain.factory_chain_id));

    //                 assert_eq!(response, test.expected_response, "{}", test.name);
    //             }
    //         }
    //     }
    // }
    // #[test]
    // fn test_query_chain() {
    //     let mut deps = mock_dependencies();

    //     // Test cases
    //     let test_cases = vec![TestQueryChain {
    //         name: "Valid query chain",
    //         chain_id: "chain1".to_string(),
    //         expected_response: ChainResponse {
    //             chain: Chain {
    //                 factory_chain_id: "factory_chain_id_1".to_string(),
    //                 factory: "factory_1".to_string(),
    //                 from_hub_channel: "hub_channel_1".to_string(),
    //                 from_factory_channel: "factory_channel_1".to_string(),
    //             },
    //             chain_uid: ChainUid::create("1".to_string()).unwrap(),
    //         },
    //         expected_error: None,
    //     }];

    //     for test in test_cases {
    //         let chain_data = Chain {
    //             factory_chain_id: test.expected_response.chain.factory_chain_id.clone(),
    //             factory: test.expected_response.chain.factory.clone(),
    //             from_hub_channel: test.expected_response.chain.from_hub_channel.clone(),
    //             from_factory_channel: test.expected_response.chain.from_factory_channel.clone(),
    //         };

    //         CHAIN_ID_TO_CHAIN
    //             .save(&mut deps.storage, test.chain_id.clone(), &chain_data)
    //             .unwrap();
    //         let res = query_chain(deps.as_ref(), test.chain_id.clone());
    //         match test.expected_error {
    //             Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
    //             None => {
    //                 let bin = res.unwrap();
    //                 let response: ChainResponse = from_json(&bin).unwrap();
    //                 assert_eq!(response, test.expected_response);
    //             }
    //         }
    //     }
    // }
    // #[test]
    // fn test_query_simulate_swap() {
    //     let mut deps = mock_dependencies();

    //     // Mock querier to return a simulated swap response
    //     deps.querier.update_wasm(|_query| {
    //         // Simulate constructing the binary response
    //         let binary_response = match to_json_binary(&GetSwapResponse {
    //             amount_out: Uint128::new(100),
    //             asset_out: Token::create("token_out".to_string()).unwrap(),
    //         }) {
    //             Ok(binary) => binary,
    //             Err(err) => {
    //                 return SystemResult::Err(SystemError::InvalidRequest {
    //                     error: format!("Failed to serialize response: {}", err),
    //                     request: Binary::default(),
    //                 });
    //             }
    //         };
    //         // Wrap the binary response in ContractResult and SystemResult
    //         let contract_result = ContractResult::Ok(binary_response);
    //         SystemResult::Ok(contract_result)
    //     });

    //     // Test cases
    //     let test_cases = vec![TestQuerySimulateSwap {
    //         name: "Valid simulate swap",
    //         msg: QuerySimulateSwap {
    //             asset_in: Token::create("token_in".to_string()).unwrap(),
    //             amount_in: Uint128::new(50),
    //             swaps: vec![NextSwapPair {
    //                 token_in: todo!(),
    //                 token_out: todo!(),
    //                 test_fail: todo!(),
    //             }],
    //             min_amount_out: Uint128::one(),
    //             asset_out: todo!(),
    //         },
    //         expected_response: SimulateSwapResponse {
    //             amount_out: Uint128::new(100),
    //             asset_out: Token::create("token_out".to_string()).unwrap(),
    //         },
    //         expected_error: None,
    //     }];

    //     for test in test_cases {
    //         let token_1 = Token::create("token_in".to_string()).unwrap();
    //         let token_2 = Token::create("token_out".to_string()).unwrap();
    //         // Register the VLP in storage
    //         VLPS.save(
    //             &mut deps.storage,
    //             (token_1.clone(), token_2.clone()),
    //             &"vlp1".to_string(),
    //         )
    //         .unwrap();

    //         ESCROW_BALANCES
    //             .save(
    //                 &mut deps.storage,
    //                 (
    //                     token_1.clone(),
    //                     ChainUid::create("chain1".to_string()).unwrap(),
    //                 ),
    //                 &Uint128::new(100000),
    //             )
    //             .unwrap();

    //         ESCROW_BALANCES
    //             .save(
    //                 &mut deps.storage,
    //                 (
    //                     token_2.clone(),
    //                     ChainUid::create("chain1".to_string()).unwrap(),
    //                 ),
    //                 &Uint128::new(100000),
    //             )
    //             .unwrap();

    //         // Mock chain data
    //         CHAIN_ID_TO_CHAIN
    //             .save(
    //                 &mut deps.storage,
    //                 "chain1".to_string(),
    //                 &Chain {
    //                     factory_chain_id: "chain1".to_string(),
    //                     factory: "factory1".to_string(),
    //                     from_hub_channel: "hub_channel1".to_string(),
    //                     from_factory_channel: "factory_channel1".to_string(),
    //                 },
    //             )
    //             .unwrap();

    //         let res = query::query_simulate_swap(*deps.as_ref().borrow(), test.msg.clone());
    //         match test.expected_error {
    //             Some(err) => assert_eq!(res.unwrap_err(), err, "{}", test.name),
    //             None => {
    //                 assert!(res.is_ok(), "{:?}", res.err());
    //                 let bin = res.unwrap();
    //                 let response: SimulateSwapResponse = from_json(&bin).unwrap();
    //                 assert_eq!(response, test.expected_response, "{}", test.name);
    //             }
    //         }
    //     }
    // }
}
