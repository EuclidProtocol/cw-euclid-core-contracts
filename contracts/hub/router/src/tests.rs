#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::contract::{execute, instantiate};
    use crate::state::{State, STATE};
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{from_json, Addr, CosmosMsg, DepsMut, IbcMsg, MessageInfo, Response};
    use euclid::chain::ChainUid;
    use euclid::error::ContractError;
    use euclid::msgs::router::{
        ExecuteMsg, InstantiateMsg, RegisterFactoryChainNative, RegisterFactoryChainType,
    };
    use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;

    struct TestExecuteMsg {
        name: &'static str,
        msg: ExecuteMsg,
        expected_error: Option<ContractError>,
    }

    fn init(deps: DepsMut, info: MessageInfo) -> Response {
        let msg = InstantiateMsg {
            relayer_contract: Addr::unchecked("relayer"),
            release_fee_recipient: Addr::unchecked("release_fee_recipient"),
            default_fee_recipient: Addr::unchecked("default_fee_recipient"),
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            concentrated_vlp_code_id: 4,
            virtual_balance_code_id: 2,
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
            admin: creator,
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            concentrated_vlp_code_id: 4,
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
            concentrated_vlp_code_id: 4,
            virtual_balance_code_id: 2,
            relayer_contract: Addr::unchecked("relayer"),
            release_fee_recipient: Addr::unchecked("release_fee_recipient"),
            default_fee_recipient: Addr::unchecked("default_fee_recipient"),
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
                        let msg: FactoryCrossChainExecuteMsg = from_json(data).unwrap();
                        assert_eq!(
                            msg,
                            FactoryCrossChainExecuteMsg::RegisterFactory {
                                chain_uid: ChainUid::create("1".to_string()).unwrap(),
                                chain_type: RegisterFactoryChainType::Native(
                                    RegisterFactoryChainNative {
                                        factory_address: "factory".to_string(),
                                        factory_chain_id: "1".to_string(),
                                    },
                                ),
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
}
