#[allow(clippy::module_inception)]
#[cfg(test)]
pub(crate) mod tests {
    #[cfg(test)]
    use crate::contract::{execute, instantiate};
    use crate::ibc::receive::reusable_internal_call;
    use crate::reply::{
        ADD_LIQUIDITY_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID, SWAP_REPLY_ID, VLP_INSTANTIATE_REPLY_ID,
        VLP_POOL_REGISTER_REPLY_ID,
    };
    use crate::state::{
        FeeState, State, ADMIN, CHAIN_UID_TO_CHAIN, ESCROW_BALANCES, FEE_STATE, LOCKED_CHAINS,
        META_TRANSACTION_CONTRACT, PENDING_RELEASE_VOUCHER, PENDING_REMOVE_LIQUIDITY,
        PENDING_SWAPS, RELAYER_CONTRACT, RELEASE_FEES, STATE, TOKEN_DENOMS,
        VIRTUAL_BALANCE_CONTRACT, VLPS,
    };
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{
        from_json, to_json_binary, Addr, Binary, ContractResult, CosmosMsg, DepsMut, MessageInfo,
        Order, Response, SystemResult, Uint128, WasmQuery,
    };
    use euclid::admin::{AdminType, EuclidAdmin};
    use euclid::chain::{Chain, ChainType, ChainUid, CosmosChain};
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::error::ContractError;
    use euclid::limit::Limit;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::hook::MetaReceive;
    use euclid::msgs::router::{
        ExecuteMsg, InstantiateMsg, ManageRouterState, RegisterFactoryChainCosmos,
        RegisterFactoryChainEvm, RegisterFactoryChainNative, RegisterFactoryChainType, TokenDenom,
    };
    use euclid::msgs::vlp::base::{GetSwapQueryResponse, PoolConfig};
    use euclid::recipient::Recipient;
    use euclid::swap::NextSwapPair;
    use euclid::token::{Pair, PairWithDenomAndAmount, TokenWithDenom, TokenWithDenomAndAmount};
    use euclid::token::{Token, TokenType};
    use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;
    use euclid_ibc::router_ibc::{
        RouterCrossChainDepositTokenExecuteMsg, RouterCrossChainExecuteMsg,
        RouterCrossChainRemoveLiquidityExecuteMsg, RouterCrossChainSwapExecuteMsg,
        RouterCrossChainTransferVoucherExecuteMsg,
    };
    use rstest::{fixture, rstest};

    // -----------------------------------------------------------------------
    // Type alias & helpers
    // -----------------------------------------------------------------------

    pub type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
        cosmwasm_std::testing::MockApi,
        MockQuerier,
    >;

    fn init(deps: DepsMut, info: MessageInfo) -> Response {
        let msg = InstantiateMsg {
            relayer_contract: Addr::unchecked("relayer"),
            release_fee_recipient: Addr::unchecked("release_fee_recipient"),
            default_fee_recipient: Addr::unchecked("default_fee_recipient"),
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            virtual_balance_code_id: 2,
        };
        instantiate(deps, mock_env(), info, msg).unwrap()
    }

    /// Fixture: deps with the router contract already instantiated.
    #[fixture]
    pub fn initialized() -> MockDeps {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        init(deps.as_mut(), message_info(&creator, &[]));
        deps
    }

    /// Fixture: deps ready for WithdrawVoucher / TransferVoucher tests.
    /// Pre-seeds VIRTUAL_BALANCE_CONTRACT, CHAIN_UID_TO_CHAIN (Native),
    /// LOCKED_CHAINS (empty), TOKEN_DENOMS (usdc → uusdc on chain1),
    /// and ESCROW_BALANCES (usdc on chain1 = 500).
    #[fixture]
    fn voucher_deps() -> MockDeps {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        init(deps.as_mut(), message_info(&creator, &[]));

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        VIRTUAL_BALANCE_CONTRACT
            .save(deps.as_mut().storage, &Addr::unchecked("virtual_balance"))
            .unwrap();
        CHAIN_UID_TO_CHAIN
            .save(
                deps.as_mut().storage,
                chain_uid.clone(),
                &Chain {
                    chain_uid: chain_uid.clone(),
                    factory_address: "factory1".to_string(),
                    chain_type: ChainType::Native {},
                },
            )
            .unwrap();
        LOCKED_CHAINS.save(deps.as_mut().storage, &vec![]).unwrap();
        TOKEN_DENOMS
            .save(
                deps.as_mut().storage,
                token.clone(),
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                }],
            )
            .unwrap();
        ESCROW_BALANCES
            .save(
                deps.as_mut().storage,
                (token.to_string(), chain_uid),
                &Uint128::new(500),
            )
            .unwrap();
        deps
    }

    /// Fixture: deps for TransferVoucher tests.
    /// Pre-seeds VIRTUAL_BALANCE_CONTRACT and TOKEN_DENOMS (usdc → empty denoms list).
    #[fixture]
    fn transfer_deps() -> MockDeps {
        let mut deps = initialized();
        seed_virtual_balance(&mut deps);
        TOKEN_DENOMS
            .save(
                deps.as_mut().storage,
                Token::create("usdc".to_string()).unwrap(),
                &vec![],
            )
            .unwrap();
        deps
    }

    /// Helper: seed VIRTUAL_BALANCE_CONTRACT with address "virtual_balance".
    pub fn seed_virtual_balance(deps: &mut MockDeps) {
        VIRTUAL_BALANCE_CONTRACT
            .save(deps.as_mut().storage, &Addr::unchecked("virtual_balance"))
            .unwrap();
    }

    /// Helper: seed CHAIN_UID_TO_CHAIN with chain_uid="chain1", factory="factory1", Native.
    pub fn seed_chain1_native(deps: &mut MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        CHAIN_UID_TO_CHAIN
            .save(
                deps.as_mut().storage,
                chain_uid.clone(),
                &Chain {
                    chain_uid: chain_uid.clone(),
                    factory_address: "factory1".to_string(),
                    chain_type: ChainType::Native {},
                },
            )
            .unwrap();
    }

    fn make_native_recipient(
        chain_uid: ChainUid,
        address: &str,
        denom: &str,
        limit: Uint128,
    ) -> euclid::recipient::Recipient {
        use euclid::recipient::Recipient;
        Recipient {
            recipient: euclid::cross_chain_user::CrossChainUser::new(
                chain_uid,
                address.to_string(),
            ),
            amount: Limit::LessThanOrEqual(limit),
            denom: TokenType::Native {
                denom: denom.to_string(),
            },
            forwarding_message: None,
            unsafe_refund_as_voucher: None,
        }
    }

    // -----------------------------------------------------------------------
    // Instantiate
    // -----------------------------------------------------------------------

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        let info = message_info(&creator, &[]);
        init(deps.as_mut(), info);
        let expected_state = State {
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            locked: false,
        };
        let state = STATE.load(deps.as_ref().storage).unwrap();
        let admins = ADMIN.load(deps.as_ref().storage).unwrap();
        assert_eq!(expected_state, state);
        assert_eq!(admins, EuclidAdmin::default(creator));
    }

    #[test]
    fn test_instantiate_state_fields() {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        let info = message_info(&creator, &[]);
        let res = init(deps.as_mut(), info.clone());

        assert_eq!(res.attributes[0].key, "method");
        assert_eq!(res.attributes[0].value, "instantiate");
        assert_eq!(res.messages.len(), 1);

        let relayer = RELAYER_CONTRACT.load(deps.as_ref().storage).unwrap();
        assert_eq!(relayer, Addr::unchecked("relayer"));

        let fee_state = FEE_STATE.load(deps.as_ref().storage).unwrap();
        assert_eq!(
            fee_state,
            FeeState {
                release_fee_recipient: Addr::unchecked("release_fee_recipient"),
                default_fee_recipient: Addr::unchecked("default_fee_recipient"),
            }
        );

        let locked_chains = LOCKED_CHAINS.load(deps.as_ref().storage).unwrap();
        assert!(locked_chains.is_empty());
    }

    // -----------------------------------------------------------------------
    // RegisterFactory: happy path
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::by_admin("creator", None)]
    #[case::by_non_admin("non-admin", Some(ContractError::Unauthorized {}))]
    fn test_execute_register_factory(
        mut initialized: MockDeps,
        #[case] sender_name: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        use cosmwasm_std::WasmMsg;
        use euclid_ibc::state::{
            NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT, NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE,
            NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE,
        };

        let chain_uid = ChainUid::create("1".to_string()).unwrap();
        let env = mock_env();
        let sender = initialized.api.addr_make(sender_name);
        let msg = ExecuteMsg::RegisterFactory {
            chain_uid: chain_uid.clone(),
            chain_info: RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                factory_address: "factory".to_string(),
                factory_chain_id: "1".to_string(),
            }),
        };
        let res = execute(initialized.as_mut(), env, message_info(&sender, &[]), msg);
        match expected_error {
            Some(err) => assert_eq!(res.unwrap_err(), err),
            None => {
                let res = res.unwrap();
                assert_eq!(res.attributes[0].key, "method");
                assert_eq!(res.attributes[0].value, "register_factory");
                assert_eq!(res.messages.len(), 1);

                // For a Native chain the SubMsg is a WasmMsg::Execute targeting the
                // factory contract (not an IBC packet — that only appears after ack).
                match &res.messages[0].msg {
                    CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr, msg, ..
                    }) => {
                        assert_eq!(contract_addr, "factory");
                        // Inner payload is NativeReceiveCallback wrapping
                        // FactoryCrossChainExecuteMsg::RegisterFactory.
                        let cb: euclid::msgs::factory::msg::ExecuteMsg = from_json(msg).unwrap();
                        if let euclid::msgs::factory::msg::ExecuteMsg::NativeReceiveCallback {
                            msg: inner,
                        } = cb
                        {
                            let factory_msg: FactoryCrossChainExecuteMsg =
                                from_json(inner).unwrap();
                            // Verify chain identity; tx_id includes the bech32 sender
                            // address generated by addr_make so we assert it separately.
                            if let FactoryCrossChainExecuteMsg::RegisterFactory {
                                chain_uid: msg_chain_uid,
                                chain_type,
                                tx_id,
                            } = factory_msg
                            {
                                assert_eq!(
                                    msg_chain_uid,
                                    ChainUid::create("1".to_string()).unwrap()
                                );
                                assert_eq!(
                                    chain_type,
                                    RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                                        factory_address: "factory".to_string(),
                                        factory_chain_id: "1".to_string(),
                                    })
                                );
                                assert!(
                                    tx_id.starts_with("vsl:"),
                                    "tx_id should be VSL-prefixed, got {tx_id}"
                                );
                            } else {
                                panic!("expected RegisterFactory inner message");
                            }
                        } else {
                            panic!("expected NativeReceiveCallback inner message");
                        }
                    }
                    other => panic!("expected WasmMsg::Execute to factory, got {other:?}"),
                }

                // CHAIN_UID_TO_CHAIN is only written after the ack arrives, not here.
                assert!(
                    !CHAIN_UID_TO_CHAIN.has(initialized.as_ref().storage, chain_uid),
                    "CHAIN_UID_TO_CHAIN must not be written before ack"
                );

                // The native IBC queue entry must be enqueued.
                let queue_count = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT
                    .load(initialized.as_ref().storage)
                    .unwrap();
                assert_eq!(
                    queue_count,
                    NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0 + 1,
                    "native queue count should be incremented by 1"
                );
                assert!(
                    NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.has(
                        initialized.as_ref().storage,
                        NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0
                    ),
                    "pending packet should be enqueued at range start"
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // RegisterFactory: validation errors (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::vsl_uid_rejected(
        ChainUid::vsl_chain_uid().unwrap(),
        RegisterFactoryChainType::Native(RegisterFactoryChainNative {
            factory_address: "factory".to_string(),
            factory_chain_id: "vsl".to_string(),
        }),
        ContractError::new("Cannot use VSL chain uid"),
    )]
    #[case::cosmos_uppercase_address(
        ChainUid::create("cosmos1".to_string()).unwrap(),
        RegisterFactoryChainType::Cosmos(RegisterFactoryChainCosmos {
            factory_address: "UPPERCASE_FACTORY".to_string(),
            factory_chain_id: "cosmos1".to_string(),
        }),
        ContractError::new("Factory address must be lowercase"),
    )]
    #[case::evm_uppercase_address(
        ChainUid::create("evm1".to_string()).unwrap(),
        RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
            factory_address: "0xUpperCase".to_string(),
            factory_chain_id: "evm1".to_string(),
        }),
        ContractError::new("Factory address must be lowercase"),
    )]
    fn test_register_factory_rejects_invalid_input(
        mut initialized: MockDeps,
        #[case] chain_uid: ChainUid,
        #[case] chain_info: RegisterFactoryChainType,
        #[case] expected_error: ContractError,
    ) {
        let creator = initialized.api.addr_make("creator");
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::RegisterFactory {
                chain_uid,
                chain_info,
            },
        );
        assert_eq!(res.unwrap_err(), expected_error);
    }

    #[rstest]
    fn test_register_factory_duplicate_chain_rejected(mut initialized: MockDeps) {
        // CHAIN_UID_TO_CHAIN is only written by the ack handler, not by the execute
        // itself.  Seed it directly to simulate a chain that already completed
        // registration, then verify execute rejects the duplicate.
        seed_chain1_native(&mut initialized);

        let creator = initialized.api.addr_make("creator");
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::RegisterFactory {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                chain_info: RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                    factory_address: "factory1".to_string(),
                    factory_chain_id: "chain1".to_string(),
                }),
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Factory already exists")
        );
    }

    // -----------------------------------------------------------------------
    // ManageRouterState: auth guard (covers all variants uniformly)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::lock_state(ManageRouterState::LockState { locked: true })]
    #[case::vlp_code_id(ManageRouterState::Vlp { vlp_code_id: Some(99), stable_vlp_code_id: None })]
    #[case::relayer_contract(ManageRouterState::RelayerContract { relayer_contract: Addr::unchecked("x") })]
    #[case::meta_transaction_contract(ManageRouterState::MetaTransactionContract { meta_transaction_contract: Addr::unchecked("x") })]
    #[case::update_fee_state(ManageRouterState::UpdateFeeState { release_fee_recipient: None, default_fee_recipient: None })]
    fn test_manage_router_state_rejects_non_admin(
        mut initialized: MockDeps,
        #[case] variant: ManageRouterState,
    ) {
        let non_admin = initialized.api.addr_make("non_admin");
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(variant),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // ManageRouterState: happy paths
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_manage_router_state_lock_state(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();
        assert!(STATE.load(initialized.as_ref().storage).unwrap().locked);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: false }),
        )
        .unwrap();
        assert!(!STATE.load(initialized.as_ref().storage).unwrap().locked);
    }

    #[rstest]
    fn test_contract_locked_blocks_execute(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterFactory {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                chain_info: RegisterFactoryChainType::Native(RegisterFactoryChainNative {
                    factory_address: "factory".to_string(),
                    factory_chain_id: "chain1".to_string(),
                }),
            },
        );
        assert_eq!(res.unwrap_err(), ContractError::ContractLocked {});

        // ManageRouterState still works while locked
        assert!(execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: false }),
        )
        .is_ok());
    }

    #[rstest]
    fn test_manage_router_state_vlp_code_id(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: None,
            }),
        )
        .unwrap();
        let state = STATE.load(initialized.as_ref().storage).unwrap();
        assert_eq!(state.constant_product_vlp_code_id, 99);
        assert_eq!(state.stable_vlp_code_id, 3); // unchanged

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: None,
                stable_vlp_code_id: Some(77),
            }),
        )
        .unwrap();
        let state = STATE.load(initialized.as_ref().storage).unwrap();
        assert_eq!(state.constant_product_vlp_code_id, 99); // unchanged
        assert_eq!(state.stable_vlp_code_id, 77);
    }

    #[rstest]
    fn test_manage_router_state_relayer_contract(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let new_relayer = initialized.api.addr_make("new_relayer");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::RelayerContract {
                relayer_contract: new_relayer.clone(),
            }),
        )
        .unwrap();
        assert_eq!(
            RELAYER_CONTRACT.load(initialized.as_ref().storage).unwrap(),
            new_relayer
        );
    }

    #[rstest]
    fn test_manage_router_state_meta_transaction_contract(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let meta_tx = initialized.api.addr_make("meta_tx_contract");
        let info = message_info(&creator, &[]);

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::MetaTransactionContract {
                meta_transaction_contract: meta_tx.clone(),
            }),
        )
        .unwrap();
        assert_eq!(res.attributes[0].value, "update_meta_transaction_contract");
        assert_eq!(res.attributes[1].value, meta_tx.to_string());
        assert_eq!(
            META_TRANSACTION_CONTRACT
                .load(initialized.as_ref().storage)
                .unwrap(),
            meta_tx
        );
    }

    #[rstest]
    fn test_manage_router_state_update_fee_state(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let new_recipient = initialized.api.addr_make("new_recipient");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateFeeState {
                release_fee_recipient: Some(new_recipient.clone()),
                default_fee_recipient: None,
            }),
        )
        .unwrap();
        assert_eq!(
            FEE_STATE
                .load(initialized.as_ref().storage)
                .unwrap()
                .release_fee_recipient,
            new_recipient
        );
    }

    #[rstest]
    fn test_manage_router_state_update_release_fee(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        let token = Token::create("usdc".to_string()).unwrap();
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateReleaseFee {
                token: token.clone(),
                chain_uid: chain_uid.clone(),
                release_fee: Uint128::new(50),
            }),
        )
        .unwrap();
        assert_eq!(
            RELEASE_FEES
                .load(initialized.as_ref().storage, (token, chain_uid))
                .unwrap(),
            Uint128::new(50)
        );
    }

    #[rstest]
    fn test_manage_router_state_update_default_release_fee(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateDefaultReleaseFee {
                default_release_fee: Uint128::new(100),
            }),
        )
        .unwrap();
        assert_eq!(res.attributes[0].value, "update_default_release_fee");
        assert_eq!(res.attributes[1].value, "100");
    }

    #[rstest]
    fn test_manage_router_state_lock_unlock_chain(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let non_admin = initialized.api.addr_make("non_admin");
        let info = message_info(&creator, &[]);

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockChain {
                chain: chain_uid.clone(),
            }),
        )
        .unwrap();
        assert!(LOCKED_CHAINS
            .load(initialized.as_ref().storage)
            .unwrap()
            .contains(&chain_uid));

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::new("Chain already locked"));

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UnlockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UnlockChain {
                chain: chain_uid.clone(),
            }),
        )
        .unwrap();
        assert!(!LOCKED_CHAINS
            .load(initialized.as_ref().storage)
            .unwrap()
            .contains(&chain_uid));

        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UnlockChain {
                chain: chain_uid.clone(),
            }),
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Chain already unlocked")
        );
    }

    #[rstest]
    fn test_manage_router_state_update_chain_timeout(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateChainTimeout {
                chain_uid: chain_uid.clone(),
                timeout: 300,
            }),
        )
        .unwrap();
        assert_eq!(res.attributes[0].value, "update_chain_timeout");
        assert_eq!(res.attributes[2].value, "300");
    }

    #[rstest]
    fn test_manage_router_state_update_admins(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let new_general_admin = initialized.api.addr_make("new_general_admin");
        let non_admin = initialized.api.addr_make("non_admin");
        let info = message_info(&creator, &[]);

        // Admins returns UnauthorizedWithMsg (not Unauthorized) — tested here separately
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&non_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general_admin.to_string(),
            }),
        );
        assert!(res.is_err());

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general_admin.to_string(),
            }),
        )
        .unwrap();
        assert_eq!(
            ADMIN
                .load(initialized.as_ref().storage)
                .unwrap()
                .general_admin,
            new_general_admin
        );
    }

    // -----------------------------------------------------------------------
    // MetaReceive: access control
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_meta_receive_rejects_unauthorized_caller(mut initialized: MockDeps) {
        let meta_tx = initialized.api.addr_make("meta_tx_contract");
        META_TRANSACTION_CONTRACT
            .save(initialized.as_mut().storage, &meta_tx)
            .unwrap();
        let attacker = Addr::unchecked("attacker");

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&attacker, &[]),
            ExecuteMsg::MetaReceive(MetaReceive {
                verified_sender: euclid::cross_chain_user::CrossChainUser::new(
                    ChainUid::vsl_chain_uid().unwrap(),
                    "user".to_string(),
                ),
                call_data: "{}".to_string(),
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[rstest]
    fn test_meta_receive_fails_when_contract_not_set(mut initialized: MockDeps) {
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&Addr::unchecked("anyone"), &[]),
            ExecuteMsg::MetaReceive(MetaReceive {
                verified_sender: euclid::cross_chain_user::CrossChainUser::new(
                    ChainUid::vsl_chain_uid().unwrap(),
                    "user".to_string(),
                ),
                call_data: "{}".to_string(),
            }),
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // NativeReceiveCallback: access control (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_native_receive_callback_unregistered_chain(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::NativeReceiveCallback {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                msg: Binary::default(),
            },
        );
        assert!(res.is_err());
    }

    #[rstest]
    #[case::non_native_chain(
        ChainType::Cosmos(CosmosChain { chain_id: "cosmos-1".to_string() }),
        "factory1",
        "factory1",
    )]
    #[case::wrong_factory_caller(
        ChainType::Native {},
        "real_factory",
        "attacker",
    )]
    fn test_native_receive_callback_unauthorized(
        mut initialized: MockDeps,
        #[case] chain_type: ChainType,
        #[case] factory_address: &str,
        #[case] caller_name: &str,
    ) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        CHAIN_UID_TO_CHAIN
            .save(
                initialized.as_mut().storage,
                chain_uid.clone(),
                &Chain {
                    chain_uid: chain_uid.clone(),
                    factory_address: factory_address.to_string(),
                    chain_type,
                },
            )
            .unwrap();

        let caller = initialized.api.addr_make(caller_name);
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&caller, &[]),
            ExecuteMsg::NativeReceiveCallback {
                chain_uid,
                msg: Binary::default(),
            },
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // Relay handlers: unauthorized callers (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::send_packet(
        Addr::unchecked("external_caller"),
        ExecuteMsg::SendPacket {
            chain: Chain {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                factory_address: "factory1".to_string(),
                chain_type: ChainType::Native {},
            },
            msg: Binary::default(),
            sender: "sender".to_string(),
            timeout: None,
            ack_response: None,
        },
    )]
    #[case::receive_packet(
        Addr::unchecked("attacker"),
        ExecuteMsg::ReceivePacket {
            source_port: "chain1.factory1".to_string(),
            destination_port: "vsl.contract".to_string(),
            msg: Binary::default(),
            sequence: 0,
            timeout: u64::MAX,
        },
    )]
    #[case::acknowledge_packet(
        Addr::unchecked("attacker"),
        ExecuteMsg::AcknowledgePacket {
            source_port: "chain1.factory1".to_string(),
            destination_port: "vsl.contract".to_string(),
            msg: Binary::default(),
            sequence: 0,
            ack: Binary::default(),
        },
    )]
    #[case::receive_packet_internal_callback(
        Addr::unchecked("external"),
        ExecuteMsg::ReceivePacketInternalCallback {
            msg: Binary::default(),
            chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
            timeout: u64::MAX,
        },
    )]
    fn test_relay_handler_rejects_unauthorized_caller(
        mut initialized: MockDeps,
        #[case] sender: Addr,
        #[case] msg: ExecuteMsg,
    ) {
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            msg,
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // ReceivePacket: port & sequence validation (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::invalid_source_port(
        "chain1.wrongfactory",
        false,
        ContractError::new("Invalid source port")
    )]
    #[case::duplicate_sequence(
        "chain1.factory1",
        true,
        ContractError::Generic { err: "Processed sequence already exists".to_string() },
    )]
    fn test_receive_packet_validation_errors(
        mut initialized: MockDeps,
        #[case] source_port: &str,
        #[case] setup_duplicate: bool,
        #[case] expected_error: ContractError,
    ) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_chain1_native(&mut initialized);

        if setup_duplicate {
            use crate::relay_state::CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS;
            CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS
                .save(
                    initialized.as_mut().storage,
                    (chain_uid.clone(), 0_u128),
                    &Uint128::from(1_u64),
                )
                .unwrap();
        }

        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("relayer"), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: source_port.to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
                msg: Binary::default(),
                sequence: 0,
                timeout: u64::MAX,
            },
        );
        assert_eq!(res.unwrap_err(), expected_error);
    }

    #[rstest]
    fn test_receive_packet_unregistered_chain_fails(mut initialized: MockDeps) {
        let env = mock_env();
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("relayer"), &[]),
            ExecuteMsg::ReceivePacket {
                source_port: "unknownchain.factory1".to_string(),
                destination_port: format!("vsl.{}", env.contract.address),
                msg: Binary::default(),
                sequence: 0,
                timeout: u64::MAX,
            },
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // AcknowledgePacket: port validation
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_acknowledge_packet_invalid_destination_port(mut initialized: MockDeps) {
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&Addr::unchecked("relayer"), &[]),
            ExecuteMsg::AcknowledgePacket {
                source_port: "chain1.factory1".to_string(),
                destination_port: "wrong.something".to_string(),
                msg: Binary::default(),
                sequence: 0,
                ack: Binary::default(),
            },
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Invalid destination port")
        );
    }

    // -----------------------------------------------------------------------
    // ReceivePacketInternalCallback: timeout
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_receive_packet_internal_callback_timed_out(mut initialized: MockDeps) {
        let env = mock_env();
        // timeout=0 is below mock_env block time (1571797419)
        let res = execute(
            initialized.as_mut(),
            env.clone(),
            message_info(&env.contract.address, &[]),
            ExecuteMsg::ReceivePacketInternalCallback {
                msg: Binary::default(),
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                timeout: 0,
            },
        );
        assert!(matches!(
            res.unwrap_err(),
            ContractError::PacketTimedOut { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // WithdrawVoucher: error cases (table-driven)
    // -----------------------------------------------------------------------

    #[rstest]
    #[case::invalid_denom("uatom", false, ContractError::InvalidDenom {})]
    #[case::locked_chain("uusdc", true, ContractError::new("Chain is locked"))]
    fn test_withdraw_voucher_error_cases(
        mut initialized: MockDeps,
        #[case] registered_denom: &str,
        #[case] lock_chain: bool,
        #[case] expected_error: ContractError,
    ) {
        let creator = initialized.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        seed_chain1_native(&mut initialized);
        let locked = if lock_chain {
            vec![chain_uid.clone()]
        } else {
            vec![]
        };
        LOCKED_CHAINS
            .save(initialized.as_mut().storage, &locked)
            .unwrap();
        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token.clone(),
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: registered_denom.to_string(),
                    },
                }],
            )
            .unwrap();

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token,
                amount: Uint128::new(100),
                recipient: make_native_recipient(
                    chain_uid,
                    "recipient",
                    "uusdc",
                    Uint128::new(100),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        );
        assert_eq!(res.unwrap_err(), expected_error);
    }

    #[rstest]
    fn test_withdraw_voucher_unregistered_token_fails(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");

        seed_virtual_balance(&mut initialized);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token: Token::create("usdc".to_string()).unwrap(),
                amount: Uint128::new(100),
                recipient: make_native_recipient(
                    ChainUid::create("chain1".to_string()).unwrap(),
                    "recipient",
                    "uusdc",
                    Uint128::new(100),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_withdraw_voucher_happy_path() {
        let mut deps = voucher_deps();
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token: token.clone(),
                amount: Uint128::new(200),
                recipient: make_native_recipient(
                    chain_uid.clone(),
                    "recipientaddr",
                    "uusdc",
                    Uint128::new(200),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        )
        .unwrap();

        assert_eq!(
            res.messages.len(),
            2,
            "expected burn + ibc release submessages"
        );

        let escrow = ESCROW_BALANCES
            .load(
                deps.as_ref().storage,
                (token.to_string(), chain_uid.clone()),
            )
            .unwrap();
        assert_eq!(escrow, Uint128::new(300));

        let pending: Vec<_> = PENDING_RELEASE_VOUCHER
            .range(deps.as_ref().storage, None, None, Order::Ascending)
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1.total_amount, Uint128::new(200));
        assert_eq!(pending[0].1.release_fee_amount, Uint128::zero());

        let released_attr = res
            .attributes
            .iter()
            .find(|a| a.key == "released_amount")
            .unwrap();
        assert_eq!(released_attr.value, "200");
    }

    #[test]
    fn test_withdraw_voucher_with_release_fee() {
        let mut deps = voucher_deps();
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        RELEASE_FEES
            .save(
                deps.as_mut().storage,
                (token.clone(), chain_uid.clone()),
                &Uint128::new(10),
            )
            .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token: token.clone(),
                amount: Uint128::new(200),
                recipient: make_native_recipient(
                    chain_uid.clone(),
                    "recipientaddr",
                    "uusdc",
                    Uint128::new(200),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        )
        .unwrap();

        // escrow reduced by release_amount_after_fee = 200 - 10 = 190
        let escrow = ESCROW_BALANCES
            .load(
                deps.as_ref().storage,
                (token.to_string(), chain_uid.clone()),
            )
            .unwrap();
        assert_eq!(escrow, Uint128::new(310));

        let pending: Vec<_> = PENDING_RELEASE_VOUCHER
            .range(deps.as_ref().storage, None, None, Order::Ascending)
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(pending[0].1.total_amount, Uint128::new(200));
        assert_eq!(pending[0].1.release_fee_amount, Uint128::new(10));
    }

    // -----------------------------------------------------------------------
    // TransferVoucher
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_transfer_voucher_unregistered_token_fails(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");

        seed_virtual_balance(&mut initialized);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::TransferVoucher {
                token: Token::create("usdc".to_string()).unwrap(),
                amount: Uint128::new(100),
                recipient: vec![euclid::recipient::Recipient {
                    recipient: euclid::cross_chain_user::CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        "recipient_addr".to_string(),
                    ),
                    amount: Limit::LessThanOrEqual(Uint128::new(100)),
                    denom: TokenType::Voucher {},
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
            },
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_transfer_voucher_to_voucher_recipient_happy_path() {
        let mut deps = transfer_deps();
        let sender = Addr::unchecked("sender_address");
        let token = Token::create("usdc".to_string()).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::TransferVoucher {
                token,
                amount: Uint128::new(100),
                recipient: vec![euclid::recipient::Recipient {
                    recipient: euclid::cross_chain_user::CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        "recipient_addr".to_string(),
                    ),
                    amount: Limit::LessThanOrEqual(Uint128::new(100)),
                    denom: TokenType::Voucher {},
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
            },
        )
        .unwrap();

        assert_eq!(
            res.messages.len(),
            1,
            "expected 1 virtual balance transfer submsg"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "transferred_amount")
                .unwrap()
                .value,
            "100"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "release_initiated")
                .unwrap()
                .value,
            "0"
        );
    }

    #[test]
    fn test_transfer_voucher_self_transfer_no_submsg() {
        let mut deps = transfer_deps();
        let sender = Addr::unchecked("senderaddr");
        let token = Token::create("usdc".to_string()).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::TransferVoucher {
                token,
                amount: Uint128::new(100),
                recipient: vec![euclid::recipient::Recipient {
                    recipient: euclid::cross_chain_user::CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        sender.to_string(),
                    ),
                    amount: Limit::LessThanOrEqual(Uint128::new(100)),
                    denom: TokenType::Voucher {},
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
            },
        )
        .unwrap();

        assert!(
            res.messages.is_empty(),
            "expected no submessages for self-transfer"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "transferred_amount")
                .unwrap()
                .value,
            "100"
        );
    }

    // -----------------------------------------------------------------------
    // reusable_internal_call: gate checks
    // -----------------------------------------------------------------------

    fn call_reusable(
        deps: &mut MockDeps,
        msg: RouterCrossChainExecuteMsg,
        chain_uid: ChainUid,
    ) -> Result<Response, ContractError> {
        let mut deps_mut = deps.as_mut();
        reusable_internal_call(
            &mut deps_mut,
            mock_env(),
            message_info(&Addr::unchecked("anyone"), &[]),
            msg,
            chain_uid,
        )
    }

    fn register_denom_msg(chain_uid: ChainUid) -> RouterCrossChainExecuteMsg {
        RouterCrossChainExecuteMsg::RegisterDenom {
            sender: CrossChainUser::new(chain_uid, "user".to_string()),
            token: TokenWithDenom {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        }
    }

    #[rstest]
    fn test_receive_dispatch_contract_locked(mut initialized: MockDeps) {
        let mut state = STATE.load(initialized.as_ref().storage).unwrap();
        state.locked = true;
        STATE.save(initialized.as_mut().storage, &state).unwrap();

        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let result = call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid,
        );
        assert_eq!(result.unwrap_err(), ContractError::ContractLocked {});
    }

    #[rstest]
    fn test_receive_dispatch_chain_locked(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        LOCKED_CHAINS
            .save(initialized.as_mut().storage, &vec![chain_uid.clone()])
            .unwrap();

        let result = call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid,
        );
        assert_eq!(result.unwrap_err(), ContractError::DeregisteredChain {});
    }

    #[rstest]
    fn test_receive_dispatch_chain_uid_mismatch(mut initialized: MockDeps) {
        let chain1 = ChainUid::create("chain1".to_string()).unwrap();
        let chain2 = ChainUid::create("chain2".to_string()).unwrap();
        // sender is from chain2 but chain_uid param is chain1
        let msg = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: CrossChainUser::new(chain2, "user".to_string()),
            token: TokenWithDenom {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        };
        let result = call_reusable(&mut initialized, msg, chain1);
        assert_eq!(
            result.unwrap_err(),
            ContractError::new("Chain UID mismatch")
        );
    }

    // -----------------------------------------------------------------------
    // RegisterDenom / DeregisterDenom dispatch
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_register_denom_saves_token_denoms(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid.clone(),
        )
        .unwrap();

        let denoms = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert_eq!(denoms.len(), 1);
        assert_eq!(denoms[0].chain_uid, chain_uid);
        assert_eq!(
            denoms[0].token_type,
            TokenType::Native {
                denom: "uusdc".to_string()
            }
        );
    }

    #[rstest]
    fn test_ibc_register_denom_duplicate_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token,
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                }],
            )
            .unwrap();

        let result = call_reusable(
            &mut initialized,
            register_denom_msg(chain_uid.clone()),
            chain_uid,
        );
        assert_eq!(result.unwrap_err(), ContractError::TokenAlreadyExist {});
    }

    #[rstest]
    fn test_ibc_deregister_denom_removes_entry(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token.clone(),
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                }],
            )
            .unwrap();

        let msg = RouterCrossChainExecuteMsg::DeregisterDenom {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            token: TokenWithDenom {
                token: token.clone(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        };
        call_reusable(&mut initialized, msg, chain_uid).unwrap();

        let denoms = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert!(
            denoms.is_empty(),
            "entry should be removed after deregister"
        );
    }

    #[rstest]
    fn test_ibc_deregister_denom_not_found_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        // No entry for this chain
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let msg = RouterCrossChainExecuteMsg::DeregisterDenom {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            token: TokenWithDenom {
                token,
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        };
        let result = call_reusable(&mut initialized, msg, chain_uid);
        assert_eq!(result.unwrap_err(), ContractError::AssetDoesNotExist {});
    }

    // -----------------------------------------------------------------------
    // DepositToken dispatch
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_deposit_token_updates_escrow_and_emits_mint(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        // execute_transfer_voucher unconditionally loads TOKEN_DENOMS for the token
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let msg =
            RouterCrossChainExecuteMsg::DepositToken(RouterCrossChainDepositTokenExecuteMsg {
                sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                asset_in: TokenWithDenom {
                    token: token.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                },
                amount_in: Uint128::new(100),
                recipients: vec![],
                tx_id: "tx1".to_string(),
            });
        let res = call_reusable(&mut initialized, msg, chain_uid.clone()).unwrap();

        let balance = ESCROW_BALANCES
            .load(initialized.as_ref().storage, (token.to_string(), chain_uid))
            .unwrap();
        assert_eq!(balance, Uint128::new(100));
        assert!(
            !res.messages.is_empty(),
            "expected virtual balance mint submessage"
        );
    }

    // -----------------------------------------------------------------------
    // TransferVoucher dispatch
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_ibc_transfer_voucher_returns_action_attribute(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let vsl_chain = ChainUid::vsl_chain_uid().unwrap();
        let msg = RouterCrossChainExecuteMsg::TransferVoucher(
            RouterCrossChainTransferVoucherExecuteMsg {
                sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                token,
                amount: Uint128::new(100),
                from: None,
                recipients: vec![Recipient {
                    recipient: CrossChainUser::new(vsl_chain, "recipient".to_string()),
                    amount: Limit::LessThanOrEqual(Uint128::new(100)),
                    denom: TokenType::Voucher {},
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                }],
                tx_id: "tx1".to_string(),
            },
        );
        let res = call_reusable(&mut initialized, msg, chain_uid).unwrap();

        assert!(
            res.attributes
                .iter()
                .any(|a| a.key == "action" && a.value == "transfer_virtual_balance"),
            "expected action=transfer_virtual_balance attribute"
        );
    }

    // -----------------------------------------------------------------------
    // RequestPoolCreation dispatch
    // -----------------------------------------------------------------------

    fn make_pool_pair(amount_a: u128, amount_b: u128) -> PairWithDenomAndAmount {
        PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                },
                amount: Uint128::new(amount_a),
            },
            token_2: TokenWithDenomAndAmount {
                token: Token::create("bbb".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "ubbb".to_string(),
                },
                amount: Uint128::new(amount_b),
            },
        }
    }

    #[rstest]
    fn test_ibc_request_pool_creation_both_tokens_new_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_virtual_balance(&mut initialized);
        // Neither token in TOKEN_DENOMS

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            tx_id: "tx1".to_string(),
            pair: make_pool_pair(100, 100),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 100,
        };
        let result = call_reusable(&mut initialized, msg, chain_uid);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Cannot create pool with two new tokens"),
            "expected two-new-tokens error"
        );
    }

    #[rstest]
    fn test_ibc_request_pool_creation_vlp_absent_instantiates(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token_a = Token::create("aaa".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token_a,
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: "uaaa".to_string(),
                    },
                }],
            )
            .unwrap();

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            tx_id: "tx1".to_string(),
            pair: make_pool_pair(100, 100),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 100,
        };
        let res = call_reusable(&mut initialized, msg, chain_uid).unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0].id, VLP_INSTANTIATE_REPLY_ID,
            "expected VLP instantiate submsg"
        );
    }

    #[rstest]
    fn test_ibc_request_pool_creation_vlp_present_registers_pool(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        for (tok, denom) in [(&token_a, "uaaa"), (&token_b, "ubbb")] {
            TOKEN_DENOMS
                .save(
                    initialized.as_mut().storage,
                    tok.clone(),
                    &vec![TokenDenom {
                        chain_uid: chain_uid.clone(),
                        token_type: TokenType::Native {
                            denom: denom.to_string(),
                        },
                    }],
                )
                .unwrap();
        }
        let pair = Pair::new(token_a, token_b).unwrap();
        VLPS.save(
            initialized.as_mut().storage,
            pair.get_tupple(),
            &Addr::unchecked("vlp_contract"),
        )
        .unwrap();

        let msg = RouterCrossChainExecuteMsg::RequestPoolCreation {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            tx_id: "tx1".to_string(),
            pair: make_pool_pair(100, 100),
            pool_config: PoolConfig::ConstantProduct {},
            slippage_tolerance_bps: 100,
        };
        let res = call_reusable(&mut initialized, msg, chain_uid).unwrap();

        assert_eq!(res.messages.len(), 1);
        assert_eq!(
            res.messages[0].id, VLP_POOL_REGISTER_REPLY_ID,
            "expected pool register submsg"
        );
    }

    // -----------------------------------------------------------------------
    // AddLiquidity dispatch
    // -----------------------------------------------------------------------

    fn seed_vlp_aaa_bbb(deps: &mut MockDeps) {
        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        VLPS.save(
            deps.as_mut().storage,
            pair.get_tupple(),
            &Addr::unchecked("vlp_contract"),
        )
        .unwrap();
    }

    #[rstest]
    fn test_ibc_add_liquidity_updates_escrow_and_emits_submsg(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        seed_vlp_aaa_bbb(&mut initialized);

        let msg = RouterCrossChainExecuteMsg::AddLiquidity {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            slippage_tolerance_bps: 100,
            pair: make_pool_pair(50, 50),
            tx_id: "tx1".to_string(),
        };
        let res = call_reusable(&mut initialized, msg, chain_uid.clone()).unwrap();

        assert_eq!(
            res.messages.last().unwrap().id,
            ADD_LIQUIDITY_REPLY_ID,
            "expected add-liquidity submsg"
        );
        let bal_a = ESCROW_BALANCES
            .load(
                initialized.as_ref().storage,
                (token_a.to_string(), chain_uid.clone()),
            )
            .unwrap();
        assert_eq!(bal_a, Uint128::new(50));
        let bal_b = ESCROW_BALANCES
            .load(
                initialized.as_ref().storage,
                (token_b.to_string(), chain_uid),
            )
            .unwrap();
        assert_eq!(bal_b, Uint128::new(50));
    }

    #[rstest]
    fn test_ibc_add_liquidity_missing_vlp_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_virtual_balance(&mut initialized);
        // No VLPS entry

        let msg = RouterCrossChainExecuteMsg::AddLiquidity {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            slippage_tolerance_bps: 100,
            pair: make_pool_pair(50, 50),
            tx_id: "tx1".to_string(),
        };
        assert!(call_reusable(&mut initialized, msg, chain_uid).is_err());
    }

    // -----------------------------------------------------------------------
    // RemoveLiquidity dispatch
    // -----------------------------------------------------------------------

    fn make_remove_liquidity_msg(chain_uid: &ChainUid, tx_id: &str) -> RouterCrossChainExecuteMsg {
        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        RouterCrossChainExecuteMsg::RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            lp_allocation: Uint128::new(100),
            pair,
            recipient: CrossChainUser::new(chain_uid.clone(), "recipient".to_string()),
            tx_id: tx_id.to_string(),
        })
    }

    #[rstest]
    fn test_ibc_remove_liquidity_saves_pending_and_emits_submsg(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_vlp_aaa_bbb(&mut initialized);

        let res = call_reusable(
            &mut initialized,
            make_remove_liquidity_msg(&chain_uid, "tx_rem"),
            chain_uid,
        )
        .unwrap();

        assert_eq!(
            res.messages.last().unwrap().id,
            REMOVE_LIQUIDITY_REPLY_ID,
            "expected remove-liquidity submsg"
        );
        assert!(
            PENDING_REMOVE_LIQUIDITY.has(initialized.as_ref().storage, "tx_rem".to_string()),
            "expected PENDING_REMOVE_LIQUIDITY entry"
        );
    }

    #[rstest]
    fn test_ibc_remove_liquidity_duplicate_tx_fails(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        seed_vlp_aaa_bbb(&mut initialized);

        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        let pending = RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
            lp_allocation: Uint128::new(100),
            pair,
            recipient: CrossChainUser::new(chain_uid.clone(), "recipient".to_string()),
            tx_id: "tx_dup".to_string(),
        };
        PENDING_REMOVE_LIQUIDITY
            .save(initialized.as_mut().storage, "tx_dup".to_string(), &pending)
            .unwrap();

        let result = call_reusable(
            &mut initialized,
            make_remove_liquidity_msg(&chain_uid, "tx_dup"),
            chain_uid,
        );
        assert_eq!(
            result.unwrap_err(),
            ContractError::new("tx already present")
        );
    }

    // -----------------------------------------------------------------------
    // Swap dispatch
    // -----------------------------------------------------------------------

    fn make_swap_deps_with_mock_querier(amount_out: u128) -> MockDeps {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        init(deps.as_mut(), message_info(&creator, &[]));

        seed_virtual_balance(&mut deps);
        seed_vlp_aaa_bbb(&mut deps);

        let token_b = Token::create("bbb".to_string()).unwrap();
        deps.querier.update_wasm(move |q| match q {
            WasmQuery::Smart { .. } => {
                let resp = GetSwapQueryResponse {
                    amount_out: Uint128::new(amount_out),
                    asset_out: token_b.clone(),
                    spread_amount: Uint128::zero(),
                    lp_fee: Uint128::zero(),
                    euclid_fee: Uint128::zero(),
                };
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
            }
            _ => panic!("unexpected wasm query in swap test"),
        });
        deps
    }

    fn make_swap_msg(chain_uid: &ChainUid, tx_id: &str) -> RouterCrossChainExecuteMsg {
        let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();
        RouterCrossChainExecuteMsg::Swap(RouterCrossChainSwapExecuteMsg {
            sender: sender.clone(),
            asset_in: TokenWithDenom {
                token: token_a.clone(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                },
            },
            amount_in: Uint128::new(100),
            asset_out: token_b.clone(),
            min_amount_out: Uint128::new(80),
            swaps: vec![NextSwapPair {
                token_in: token_a,
                token_out: token_b,
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee_amount: Uint128::zero(),
            partner_fee_recipient: sender,
            tx_id: tx_id.to_string(),
        })
    }

    #[test]
    fn test_ibc_swap_saves_pending_and_emits_submsg() {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let mut deps = make_swap_deps_with_mock_querier(90);
        let token_a = Token::create("aaa".to_string()).unwrap();

        let res = call_reusable(
            &mut deps,
            make_swap_msg(&chain_uid, "tx_swap"),
            chain_uid.clone(),
        )
        .unwrap();

        assert_eq!(
            res.messages.last().unwrap().id,
            SWAP_REPLY_ID,
            "expected swap submsg"
        );
        assert!(
            PENDING_SWAPS.has(deps.as_ref().storage, "tx_swap".to_string()),
            "expected PENDING_SWAPS entry"
        );
        let escrow = ESCROW_BALANCES
            .load(deps.as_ref().storage, (token_a.to_string(), chain_uid))
            .unwrap();
        assert_eq!(
            escrow,
            Uint128::new(100),
            "escrow balance should equal amount_in"
        );
    }

    #[test]
    fn test_ibc_swap_slippage_exceeded() {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        // amount_out=10 < min_amount_out=80
        let mut deps = make_swap_deps_with_mock_querier(10);

        let result = call_reusable(&mut deps, make_swap_msg(&chain_uid, "tx_slip"), chain_uid);
        assert!(
            matches!(result.unwrap_err(), ContractError::SlippageExceeded { .. }),
            "expected SlippageExceeded"
        );
    }

    // -----------------------------------------------------------------------
    // Role separation: migration_admin / fee_admin / general_admin
    // -----------------------------------------------------------------------

    /// After changing general_admin, the new address can lock state but the old
    /// one is rejected; the old address still controls migration_admin operations.
    #[rstest]
    fn test_general_admin_change_locks_out_old_admin(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");
        let new_general = initialized.api.addr_make("new_general");

        // creator delegates general_admin role.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general.to_string(),
            }),
        )
        .unwrap();

        // Old general_admin (creator) can no longer lock state.
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // New general_admin can lock state.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&new_general, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();
        assert!(STATE.load(initialized.as_ref().storage).unwrap().locked);
    }

    /// VLP code-id updates require migration_admin — general_admin alone is not enough.
    #[rstest]
    fn test_vlp_update_requires_migration_admin(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");
        let new_general = initialized.api.addr_make("new_general");

        // Give general_admin role to a different address; creator keeps migration_admin.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general.to_string(),
            }),
        )
        .unwrap();

        // new_general cannot update VLP code IDs.
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&new_general, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: None,
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // creator (still migration_admin) succeeds.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: None,
            }),
        )
        .unwrap();
        assert_eq!(
            STATE
                .load(initialized.as_ref().storage)
                .unwrap()
                .constant_product_vlp_code_id,
            99
        );
    }

    /// Fee operations require fee_admin; general_admin (when distinct) is rejected.
    #[rstest]
    fn test_fee_update_requires_fee_admin(mut initialized: MockDeps) {
        let creator = initialized.api.addr_make("creator");
        let dedicated_fee_admin = initialized.api.addr_make("dedicated_fee_admin");
        let new_general = initialized.api.addr_make("new_general");

        // Separate fee_admin (creator is currently fee_admin so can transfer it).
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::FeeAdmin,
                admin: dedicated_fee_admin.to_string(),
            }),
        )
        .unwrap();
        // Also change general_admin so creator is no longer general_admin.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::Admins {
                admin_type: AdminType::GeneralAdmin,
                admin: new_general.to_string(),
            }),
        )
        .unwrap();

        let dummy_addr = initialized.api.addr_make("r");

        // general_admin (new_general) cannot update fee state.
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&new_general, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateFeeState {
                release_fee_recipient: Some(dummy_addr),
                default_fee_recipient: None,
            }),
        );
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});

        // dedicated_fee_admin succeeds.
        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&dedicated_fee_admin, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateReleaseFee {
                token: Token::create("usdc".to_string()).unwrap(),
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                release_fee: Uint128::new(20),
            }),
        )
        .unwrap();
    }

    /// UpdateDefaultReleaseFee and UpdateChainTimeout carry no auth guard —
    /// any caller can invoke them.
    #[rstest]
    fn test_unguarded_manage_variants_accept_any_caller(mut initialized: MockDeps) {
        let random = initialized.api.addr_make("random_caller");
        let creator = initialized.api.addr_make("creator");

        // UpdateDefaultReleaseFee: no require guard in the contract.
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&random, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateDefaultReleaseFee {
                default_release_fee: Uint128::new(42),
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateDefaultReleaseFee {
                default_release_fee: Uint128::new(42),
            }),
        )
        .unwrap();

        // UpdateChainTimeout: no auth guard either.
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&random, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateChainTimeout {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                timeout: 600,
            }),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::ManageRouterState(ManageRouterState::UpdateChainTimeout {
                chain_uid: ChainUid::create("chain1".to_string()).unwrap(),
                timeout: 600,
            }),
        )
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // WithdrawVoucher: escrow-cap and Limit variants
    // -----------------------------------------------------------------------

    /// When the requested amount exceeds the escrow balance the release is
    /// silently capped at the available escrow.
    #[test]
    fn test_withdraw_voucher_capped_at_escrow_balance() {
        let mut deps = voucher_deps(); // escrow = 500
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token: token.clone(),
                amount: Uint128::new(1_000),
                recipient: make_native_recipient(
                    chain_uid.clone(),
                    "recipientaddr",
                    "uusdc",
                    Uint128::new(1_000),
                ),
                cross_chain_config: CrossChainConfig::default(),
            },
        )
        .unwrap();

        // Escrow fully drained (not negative).
        let escrow = ESCROW_BALANCES
            .load(deps.as_ref().storage, (token.to_string(), chain_uid))
            .unwrap();
        assert_eq!(escrow, Uint128::zero());

        // PENDING_RELEASE records the actual released amount (500), not the requested 1000.
        let pending: Vec<_> = PENDING_RELEASE_VOUCHER
            .range(deps.as_ref().storage, None, None, Order::Ascending)
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(pending[0].1.total_amount, Uint128::new(500));

        // released_amount attribute also reflects the cap.
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "released_amount")
                .unwrap()
                .value,
            "500"
        );
    }

    /// GreaterThanOrEqual limit fails when the escrow can't satisfy the minimum.
    #[test]
    fn test_withdraw_voucher_gte_limit_fails_when_escrow_too_low() {
        let mut deps = voucher_deps(); // escrow = 500
        let creator = deps.api.addr_make("creator");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        // Request 200 but require at least 600 (escrow=500 < 600).
        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&creator, &[]),
            ExecuteMsg::WithdrawVoucher {
                token,
                amount: Uint128::new(200),
                recipient: Recipient {
                    recipient: CrossChainUser::new(chain_uid, "recipientaddr".to_string()),
                    amount: Limit::GreaterThanOrEqual(Uint128::new(600)),
                    denom: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                    forwarding_message: None,
                    unsafe_refund_as_voucher: None,
                },
                cross_chain_config: CrossChainConfig::default(),
            },
        );
        assert!(matches!(
            res.unwrap_err(),
            ContractError::InsufficientAmount { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Multi-chain token registration
    // -----------------------------------------------------------------------

    /// The same token may be registered on two different chains simultaneously.
    #[rstest]
    fn test_register_denom_same_token_two_chains(mut initialized: MockDeps) {
        let chain1 = ChainUid::create("chain1".to_string()).unwrap();
        let chain2 = ChainUid::create("chain2".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        for (chain_uid, denom) in [(&chain1, "uusdc"), (&chain2, "usdc.ibc")] {
            call_reusable(
                &mut initialized,
                RouterCrossChainExecuteMsg::RegisterDenom {
                    sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                    token: TokenWithDenom {
                        token: token.clone(),
                        token_type: TokenType::Native {
                            denom: denom.to_string(),
                        },
                    },
                    tx_id: format!("tx_{denom}"),
                },
                chain_uid.clone(),
            )
            .unwrap();
        }

        let entries = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.chain_uid == chain1));
        assert!(entries.iter().any(|e| e.chain_uid == chain2));
    }

    /// Deregistering a token on chain1 must not affect chain2's registration.
    #[rstest]
    fn test_deregister_denom_preserves_other_chain_entries(mut initialized: MockDeps) {
        let chain1 = ChainUid::create("chain1".to_string()).unwrap();
        let chain2 = ChainUid::create("chain2".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        // Pre-seed both chains.
        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token.clone(),
                &vec![
                    TokenDenom {
                        chain_uid: chain1.clone(),
                        token_type: TokenType::Native {
                            denom: "uusdc".to_string(),
                        },
                    },
                    TokenDenom {
                        chain_uid: chain2.clone(),
                        token_type: TokenType::Native {
                            denom: "usdc.ibc".to_string(),
                        },
                    },
                ],
            )
            .unwrap();

        // Deregister only chain1.
        call_reusable(
            &mut initialized,
            RouterCrossChainExecuteMsg::DeregisterDenom {
                sender: CrossChainUser::new(chain1.clone(), "user".to_string()),
                token: TokenWithDenom {
                    token: token.clone(),
                    token_type: TokenType::Native {
                        denom: "uusdc".to_string(),
                    },
                },
                tx_id: "tx1".to_string(),
            },
            chain1.clone(),
        )
        .unwrap();

        let entries = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token)
            .unwrap();
        assert_eq!(entries.len(), 1, "chain1 entry should be removed");
        assert_eq!(entries[0].chain_uid, chain2, "chain2 entry must survive");
    }

    // -----------------------------------------------------------------------
    // DepositToken: escrow accumulation
    // -----------------------------------------------------------------------

    /// Successive deposits for the same token+chain accumulate in ESCROW_BALANCES.
    #[rstest]
    fn test_ibc_deposit_token_accumulates_balance(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token = Token::create("usdc".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        TOKEN_DENOMS
            .save(initialized.as_mut().storage, token.clone(), &vec![])
            .unwrap();

        let deposit = |deps: &mut MockDeps, amount: u128| {
            call_reusable(
                deps,
                RouterCrossChainExecuteMsg::DepositToken(RouterCrossChainDepositTokenExecuteMsg {
                    sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                    asset_in: TokenWithDenom {
                        token: token.clone(),
                        token_type: TokenType::Native {
                            denom: "uusdc".to_string(),
                        },
                    },
                    amount_in: Uint128::new(amount),
                    recipients: vec![],
                    tx_id: format!("tx_{amount}"),
                }),
                chain_uid.clone(),
            )
            .unwrap()
        };

        deposit(&mut initialized, 100);
        deposit(&mut initialized, 250);

        let balance = ESCROW_BALANCES
            .load(initialized.as_ref().storage, (token.to_string(), chain_uid))
            .unwrap();
        assert_eq!(balance, Uint128::new(350), "deposits should accumulate");
    }

    // -----------------------------------------------------------------------
    // RequestPoolCreation: new denom auto-registered in TOKEN_DENOMS
    // -----------------------------------------------------------------------

    /// When a pool is created with a brand-new token (token_b is new), the handler
    /// must add token_b's denom entry to TOKEN_DENOMS before dispatching to the VLP.
    #[rstest]
    fn test_ibc_request_pool_creation_auto_registers_new_denom(mut initialized: MockDeps) {
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();

        seed_virtual_balance(&mut initialized);
        // token_a is known; token_b is brand-new.
        TOKEN_DENOMS
            .save(
                initialized.as_mut().storage,
                token_a.clone(),
                &vec![TokenDenom {
                    chain_uid: chain_uid.clone(),
                    token_type: TokenType::Native {
                        denom: "uaaa".to_string(),
                    },
                }],
            )
            .unwrap();

        call_reusable(
            &mut initialized,
            RouterCrossChainExecuteMsg::RequestPoolCreation {
                sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                tx_id: "tx1".to_string(),
                pair: make_pool_pair(100, 100),
                pool_config: PoolConfig::ConstantProduct {},
                slippage_tolerance_bps: 100,
            },
            chain_uid.clone(),
        )
        .unwrap();

        // token_b must now be registered in TOKEN_DENOMS for chain1.
        let denoms = TOKEN_DENOMS
            .load(initialized.as_ref().storage, token_b)
            .unwrap();
        assert_eq!(denoms.len(), 1);
        assert_eq!(denoms[0].chain_uid, chain_uid);
        assert_eq!(
            denoms[0].token_type,
            TokenType::Native {
                denom: "ubbb".to_string()
            }
        );
    }

    // -----------------------------------------------------------------------
    // MetaReceive: dispatch business logic
    // -----------------------------------------------------------------------

    fn meta_receive_msg(
        verified_sender: CrossChainUser,
        inner: &RouterCrossChainExecuteMsg,
    ) -> ExecuteMsg {
        let call_data = String::from_utf8(to_json_binary(inner).unwrap().to_vec()).unwrap();
        ExecuteMsg::MetaReceive(MetaReceive {
            verified_sender,
            call_data,
        })
    }

    fn seed_meta_tx_contract(deps: &mut MockDeps) -> Addr {
        let meta_tx = deps.api.addr_make("meta_tx");
        META_TRANSACTION_CONTRACT
            .save(deps.as_mut().storage, &meta_tx)
            .unwrap();
        meta_tx
    }

    /// MetaReceive rejects message types other than Swap and TransferVoucher.
    #[rstest]
    fn test_meta_receive_unsupported_msg_type_fails(mut initialized: MockDeps) {
        let meta_tx = seed_meta_tx_contract(&mut initialized);
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());

        let inner = RouterCrossChainExecuteMsg::RegisterDenom {
            sender: sender.clone(),
            token: TokenWithDenom {
                token: Token::create("usdc".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uusdc".to_string(),
                },
            },
            tx_id: "tx1".to_string(),
        };

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&meta_tx, &[]),
            meta_receive_msg(sender, &inner),
        );
        assert!(matches!(res.unwrap_err(), ContractError::Generic { .. }));
    }

    /// MetaReceive for Swap requires the inner asset_in to be a voucher token.
    #[rstest]
    fn test_meta_receive_swap_requires_voucher_asset_in(mut initialized: MockDeps) {
        let meta_tx = seed_meta_tx_contract(&mut initialized);
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();

        let swap = RouterCrossChainSwapExecuteMsg {
            sender: sender.clone(),
            // Native denom — not a voucher.
            asset_in: TokenWithDenom {
                token: token_a.clone(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                },
            },
            amount_in: Uint128::new(100),
            asset_out: token_b,
            min_amount_out: Uint128::new(80),
            swaps: vec![NextSwapPair {
                token_in: token_a,
                token_out: Token::create("bbb".to_string()).unwrap(),
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee_amount: Uint128::zero(),
            partner_fee_recipient: sender.clone(),
            tx_id: "tx_meta_swap".to_string(),
        };

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&meta_tx, &[]),
            meta_receive_msg(sender, &RouterCrossChainExecuteMsg::Swap(swap)),
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Asset IN does not match asset OUT")
        );
    }

    /// MetaReceive for Swap rejects when verified_sender address differs from swap.sender.
    #[rstest]
    fn test_meta_receive_swap_sender_address_mismatch_fails(mut initialized: MockDeps) {
        let meta_tx = seed_meta_tx_contract(&mut initialized);
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let real_sender = CrossChainUser::new(chain_uid.clone(), "real_user".to_string());
        let impersonated = CrossChainUser::new(chain_uid.clone(), "victim".to_string());
        let token_a = Token::create("aaa".to_string()).unwrap();
        let token_b = Token::create("bbb".to_string()).unwrap();

        let swap = RouterCrossChainSwapExecuteMsg {
            // Swap claims to come from "victim".
            sender: impersonated,
            asset_in: TokenWithDenom {
                token: token_a.clone(),
                token_type: TokenType::Voucher {},
            },
            amount_in: Uint128::new(100),
            asset_out: token_b,
            min_amount_out: Uint128::new(80),
            swaps: vec![NextSwapPair {
                token_in: token_a,
                token_out: Token::create("bbb".to_string()).unwrap(),
                test_fail: None,
            }],
            recipients: vec![],
            partner_fee_amount: Uint128::zero(),
            partner_fee_recipient: real_sender.clone(),
            tx_id: "tx_mismatch".to_string(),
        };

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            message_info(&meta_tx, &[]),
            // verified_sender is "real_user" but swap.sender is "victim".
            meta_receive_msg(real_sender, &RouterCrossChainExecuteMsg::Swap(swap)),
        );
        assert_eq!(
            res.unwrap_err(),
            ContractError::new("Sender address mismatch")
        );
    }

    // -----------------------------------------------------------------------
    // TransferVoucher: multi-recipient sequential allocation
    // -----------------------------------------------------------------------

    /// When multiple recipients are listed, amount flows through them in order;
    /// each recipient absorbs up to its limit and the remainder goes to the next.
    #[test]
    fn test_transfer_voucher_splits_amount_across_recipients() {
        let mut deps = transfer_deps();
        let sender = Addr::unchecked("sender_address");
        let token = Token::create("usdc".to_string()).unwrap();
        let vsl_chain = ChainUid::vsl_chain_uid().unwrap();

        // recipient1 takes up to 60, recipient2 takes the remainder (40).
        let res = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::TransferVoucher {
                token,
                amount: Uint128::new(100),
                recipient: vec![
                    Recipient {
                        recipient: CrossChainUser::new(vsl_chain.clone(), "addr_one".to_string()),
                        amount: Limit::LessThanOrEqual(Uint128::new(60)),
                        denom: TokenType::Voucher {},
                        forwarding_message: None,
                        unsafe_refund_as_voucher: None,
                    },
                    Recipient {
                        recipient: CrossChainUser::new(vsl_chain, "addr_two".to_string()),
                        amount: Limit::LessThanOrEqual(Uint128::new(100)),
                        denom: TokenType::Voucher {},
                        forwarding_message: None,
                        unsafe_refund_as_voucher: None,
                    },
                ],
            },
        )
        .unwrap();

        // Two separate virtual-balance transfer submessages.
        assert_eq!(
            res.messages.len(),
            2,
            "expected one submsg per non-self recipient"
        );
        // Amounts reflected in attributes.
        let attrs: std::collections::HashMap<_, _> = res
            .attributes
            .iter()
            .map(|a| (a.key.as_str(), a.value.as_str()))
            .collect();
        assert_eq!(attrs["transfer_id_0_amount"], "60");
        assert_eq!(attrs["transfer_id_1_amount"], "40");
        assert_eq!(attrs["transferred_amount"], "100");
    }

    // -----------------------------------------------------------------------
    // State invariant
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_state_invariant_after_manage_sequence(mut initialized: MockDeps) {
        let env = mock_env();
        let creator = initialized.api.addr_make("creator");
        let info = message_info(&creator, &[]);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::Vlp {
                vlp_code_id: Some(99),
                stable_vlp_code_id: Some(88),
            }),
        )
        .unwrap();
        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: true }),
        )
        .unwrap();

        let state = STATE.load(initialized.as_ref().storage).unwrap();
        assert_eq!(state.constant_product_vlp_code_id, 99);
        assert_eq!(state.stable_vlp_code_id, 88);
        assert!(state.locked);

        execute(
            initialized.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ManageRouterState(ManageRouterState::LockState { locked: false }),
        )
        .unwrap();
        assert!(!STATE.load(initialized.as_ref().storage).unwrap().locked);
    }
}
