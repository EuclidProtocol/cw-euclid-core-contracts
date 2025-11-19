#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::contract::{execute, instantiate};
    use crate::state::{State, CHAIN_UID_TO_CHAIN, STATE};
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{from_json, CosmosMsg, DepsMut, IbcMsg, MessageInfo, Response};
    use euclid::chain::{Chain, ChainUid, IbcChain};
    use euclid::error::ContractError;
    use euclid::msgs::router::{
        ExecuteMsg, InstantiateMsg, RegisterFactoryChainNative, UpdateRouterState,
    };
    use euclid_ibc::msg::HubIbcExecuteMsg;

    struct TestExecuteMsg {
        name: &'static str,
        msg: ExecuteMsg,
        expected_error: Option<ContractError>,
    }

    fn init(deps: DepsMut, info: MessageInfo) -> Response {
        let msg = InstantiateMsg {
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            virtual_balance_code_id: 2,
            mock_relayer_addresses: None,
        };
        instantiate(deps, mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        let info = message_info(&creator, &[]);
        init(deps.as_mut(), info);
        let expected_state = State {
            admin: creator.to_string(),
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            virtual_balance_address: None,
            locked: false,
        };
        let state = STATE.load(deps.as_ref().storage).unwrap();

        assert_eq!(expected_state, state)
    }

    #[test]
    fn test_execute_register_factory() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let creator = deps.api.addr_make("creator");
        let non_admin = deps.api.addr_make("non-admin");
        let info = message_info(&creator, &[]);

        // Instantiate the contract first
        let msg = InstantiateMsg {
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            virtual_balance_code_id: 2,
            mock_relayer_addresses: None,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let test_cases = vec![
            TestExecuteMsg {
                name: "Register factory by admin",
                msg: ExecuteMsg::RegisterFactory {
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                    chain_info: euclid::msgs::router::RegisterFactoryChainType::Native(
                        RegisterFactoryChainNative {
                            factory_address: "factory".to_string(),
                            factory_chain_id: "1".to_string(),
                        },
                    ),
                },
                expected_error: None,
            },
            TestExecuteMsg {
                name: "Register factory by non-admin",
                msg: ExecuteMsg::RegisterFactory {
                    chain_info: euclid::msgs::router::RegisterFactoryChainType::Native(
                        RegisterFactoryChainNative {
                            factory_address: "factory".to_string(),
                            factory_chain_id: "1".to_string(),
                        },
                    ),
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                },
                expected_error: Some(ContractError::Unauthorized {}),
            },
        ];

        for test in test_cases {
            let res = execute(
                deps.as_mut(),
                env.clone(),
                if test.name.contains("non-admin") {
                    message_info(&non_admin, &[])
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
                    assert_eq!(res.attributes[0].key, "method");
                    assert_eq!(res.attributes[0].value, "register_factory");

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
                                tx_id: "vsl:creator:cosmos-testnet-14002:12345:3:1".to_string(),
                            }
                        );
                    } else {
                        //    Its a native chain call
                    }
                }
            }
        }
    }

    #[test]
    fn test_update_lock() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let owner = deps.api.addr_make("owner");
        let info = message_info(&owner, &[]);
        init(deps.as_mut(), info);

        // Unauthorized
        let msg = ExecuteMsg::UpdateLock {};
        let not_owner = deps.api.addr_make("not_owner");
        let info = message_info(&not_owner, &[]);
        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // works
        let info = message_info(&owner, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let state = STATE.load(deps.as_ref().storage).unwrap();
        assert!(state.locked);

        // Try to call a function while locked
        let msg = ExecuteMsg::UpdateFactoryChannel {
            chain_uid: ChainUid::create("uid".to_string()).unwrap(),
            channel: "channel".to_string(),
        };
        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
        assert_eq!(err, ContractError::ContractLocked {});

        // Test unlock
        let info = message_info(&owner, &[]);
        let msg = ExecuteMsg::UpdateLock {};
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        let state = STATE.load(deps.as_ref().storage).unwrap();
        assert!(!state.locked);

        // Now try calling the earlier message
        let msg = ExecuteMsg::UpdateFactoryChannel {
            chain_uid: ChainUid::create("uid".to_string()).unwrap(),
            channel: "channel".to_string(),
        };
        CHAIN_UID_TO_CHAIN
            .save(
                deps.as_mut().storage,
                ChainUid::create("uid".to_string()).unwrap(),
                &Chain {
                    factory_chain_id: "1".to_string(),
                    factory: "factory".to_string(),
                    chain_type: euclid::chain::ChainType::Ibc(IbcChain {
                        from_hub_channel: "5".to_string(),
                        from_factory_channel: "6".to_string(),
                    }),
                },
            )
            .unwrap();
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    #[test]
    fn test_update_router_state() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let owner = deps.api.addr_make("owner");
        let info = message_info(&owner, &[]);
        init(deps.as_mut(), info);
        let new_admin = deps.api.addr_make("new_admin");
        let new_virtual_balance_address = deps.api.addr_make("new_virtual_balance_address");
        let new_mock_relayer_address = deps.api.addr_make("new_mock_relayer_address");
        let new_meta_transaction_contract = deps.api.addr_make("new_meta_transaction_contract");
        // Unauthorized
        let msg = ExecuteMsg::UpdateRouterState(UpdateRouterState {
            admin: Some(new_admin.to_string()),
            vlp_code_id: Some(1),
            stable_vlp_code_id: Some(0),
            virtual_balance_address: Some(new_virtual_balance_address.clone()),
            locked: Some(true),
            mock_relayer_addresses: Some(vec![new_mock_relayer_address.to_string()]),
            meta_transaction_contract: Some(new_meta_transaction_contract.clone()),
        });
        let not_owner = deps.api.addr_make("not_owner");
        let info = message_info(&not_owner, &[]);
        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        // Works
        let info = message_info(&owner, &[]);
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        let state = STATE.load(deps.as_ref().storage).unwrap();
        assert_eq!(state.admin, new_admin.to_string());
        assert_eq!(state.constant_product_vlp_code_id, 1);
        assert_eq!(
            state.virtual_balance_address,
            Some(new_virtual_balance_address.clone())
        );
        assert!(state.locked);
    }
}
