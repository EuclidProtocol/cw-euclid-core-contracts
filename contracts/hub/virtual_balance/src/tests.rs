#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {

    use crate::contract::{execute, instantiate};
    use crate::state::{ADMIN, ALLOWANCES, BALANCES, STATE};

    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{Addr, MessageInfo, Response, Uint128};
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::error::ContractError;
    use euclid::msgs::virtual_balance::msg::{
        Allowance, ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteMsg, ExecuteTransfer,
        InstantiateMsg, State,
    };
    use euclid::voucher::BalanceKey;

    fn init(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            MockQuerier,
        >,
    ) -> Response {
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            admin: None,
        };
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
    }

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);
        assert_eq!(0, res.messages.len());
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(router.clone());

        let expected_state = State { router };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
        let saved_admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved_admin, admin);
    }

    #[test]
    fn test_mint_burn_transfer() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        // Unauthorized sender
        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);
        let cross_chain_user = CrossChainUser {
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            address: "cross_chain_user_address".to_string(),
        };
        let balance_key = BalanceKey {
            cross_chain_user: cross_chain_user.clone(),
            token_id: "token1".to_string(),
        };

        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint128::new(10_u128),
            balance_key: balance_key.clone(),
        });

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let expected_snapshot_balance = Uint128::new(10_u128);

        let key = balance_key.clone().to_serialized_balance_key();
        let snapshot_balance = BALANCES.load(&deps.storage, key).unwrap();

        assert_eq!(expected_snapshot_balance, snapshot_balance);

        // Invalid zero amount
        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint128::zero(),
            balance_key: balance_key.clone(),
        });

        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(ContractError::ZeroAssetAmount {}, err);

        // Burn //

        let msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: Uint128::new(5_u128),
            balance_key: balance_key.clone(),
        });

        // Unauthorized sender
        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);
        let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
        assert_eq!(ContractError::Unauthorized {}, err);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let expected_snapshot_balance = Uint128::new(5_u128);

        let key = balance_key.clone().to_serialized_balance_key();
        let snapshot_balance = BALANCES.load(&deps.storage, key).unwrap();

        assert_eq!(expected_snapshot_balance, snapshot_balance);

        // Zero burn amount
        let msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: Uint128::zero(),
            balance_key: balance_key.clone(),
        });

        let err = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
        assert_eq!(ContractError::ZeroAssetAmount {}, err);

        // Transfer //

        let cross_chain_user_2 = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "cross_chain_user_address_2".to_string(),
        );

        let balance_key_2 = BalanceKey {
            cross_chain_user: cross_chain_user_2.clone(),
            token_id: "token1".to_string(),
        };

        let msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: Uint128::new(2_u128),
            token_id: "token1".to_string(),
            sender: Some(cross_chain_user),
            to: cross_chain_user_2,
            from: None,
            msg: None,
        });

        let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        let expected_snapshot_balance_user_1 = Uint128::new(3_u128);
        let expected_snapshot_balance_user_2 = Uint128::new(2_u128);

        let key = balance_key.clone().to_serialized_balance_key();
        let key_2 = balance_key_2.clone().to_serialized_balance_key();

        let snapshot_balance = BALANCES.load(&deps.storage, key).unwrap();
        let snapshot_balance_2 = BALANCES.load(&deps.storage, key_2).unwrap();

        assert_eq!(expected_snapshot_balance_user_1, snapshot_balance);
        assert_eq!(expected_snapshot_balance_user_2, snapshot_balance_2);
    }

    #[test]
    fn test_allowance_and_transfer() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Setup initial state
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(router.clone());
        let state = State {
            router: router.clone(),
        };
        STATE.save(&mut deps.storage, &state).unwrap();
        ADMIN.save(&mut deps.storage, &admin).unwrap();

        // Setup users
        let owner = CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "owner".to_string());
        let spender =
            CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "spender".to_string());
        let recipient = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "recipient".to_string(),
        );

        // Mint tokens to owner
        let balance_key = BalanceKey {
            cross_chain_user: owner.clone(),
            token_id: "eucl".to_string(),
        };
        let mint_msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint128::new(20),
            balance_key: balance_key.clone(),
        });
        let info = MessageInfo {
            sender: router.clone(),
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info, mint_msg).unwrap();

        // Owner approves spender
        let approve_msg = ExecuteMsg::Approve(ExecuteApprove {
            amount: Uint128::new(10),
            token_id: "eucl".to_string(),
            spender: spender.clone(),
            owner: owner.clone(),
        });
        let info = MessageInfo {
            sender: router.clone(),
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info, approve_msg).unwrap();

        // Verify allowance was set
        let allowance = ALLOWANCES
            .load(
                &deps.storage,
                balance_key.clone().to_serialized_balance_key(),
            )
            .unwrap();
        assert_eq!(allowance.amount, Uint128::new(10));
        assert_eq!(allowance.spender, spender);

        // Spender transfers tokens to recipient
        let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: Uint128::new(5),
            token_id: "eucl".to_string(),
            from: Some(owner.clone()),
            to: recipient.clone(),
            sender: None,
            msg: None,
        });
        let info = MessageInfo {
            sender: Addr::unchecked(spender.address.clone()),
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info, transfer_msg).unwrap();

        // Spender transfers tokens to recipient
        let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: Uint128::new(5),
            token_id: "eucl".to_string(),
            from: Some(owner.clone()),
            to: recipient.clone(),
            sender: Some(spender.clone()),
            msg: None,
        });
        let info = MessageInfo {
            sender: Addr::unchecked(spender.address.clone()),
            funds: vec![],
        };
        // Unauthorized error as only router can set pseudo sender
        let err = execute(deps.as_mut(), env.clone(), info, transfer_msg).unwrap_err();
        assert_eq!(
            ContractError::UnauthorizedWithMsg {
                msg: "Only router can set pseudo sender".to_string()
            },
            err
        );

        // Verify balances after transfer
        let owner_balance = BALANCES
            .load(
                &deps.storage,
                balance_key.clone().to_serialized_balance_key(),
            )
            .unwrap();
        assert_eq!(owner_balance, Uint128::new(15));

        let recipient_key = BalanceKey {
            cross_chain_user: recipient.clone(),
            token_id: "eucl".to_string(),
        };
        let recipient_balance = BALANCES
            .load(
                &deps.storage,
                recipient_key.clone().to_serialized_balance_key(),
            )
            .unwrap();
        assert_eq!(recipient_balance, Uint128::new(5));

        // Check spender's allowance
        let spender_allowance = ALLOWANCES
            .load(
                &deps.storage,
                balance_key.clone().to_serialized_balance_key(),
            )
            .unwrap();
        assert_eq!(spender_allowance.amount, Uint128::new(5));

        // Spender transfers his remaining tokens to recipient
        let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: Uint128::new(5),
            token_id: "eucl".to_string(),
            from: Some(owner.clone()),
            to: recipient.clone(),
            sender: None,
            msg: None,
        });
        let info = MessageInfo {
            sender: Addr::unchecked(spender.address.clone()),
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info, transfer_msg).unwrap();

        // Check that spender's the allowance has been removed
        let _spender_allowance = ALLOWANCES
            .load(
                &deps.storage,
                balance_key.clone().to_serialized_balance_key(),
            )
            .unwrap_err();

        // Check recipient's balance
        let recipient_balance = BALANCES
            .load(
                &deps.storage,
                recipient_key.clone().to_serialized_balance_key(),
            )
            .unwrap();
        assert_eq!(recipient_balance, Uint128::new(10));

        // Burn the recipient's remaining balance
        let burn_msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: recipient_balance,
            balance_key: recipient_key.clone(),
        });
        let info = MessageInfo {
            sender: Addr::unchecked(router.clone()),
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info, burn_msg).unwrap();

        // Check that recipient's balance has been removed
        let _recipient_balance = BALANCES
            .load(
                &deps.storage,
                recipient_key.clone().to_serialized_balance_key(),
            )
            .unwrap_err();
    }

    #[test]
    fn test_remove_zero_state_values() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(router.clone());

        // Save initial state
        STATE.save(&mut deps.storage, &State { router }).unwrap();
        ADMIN.save(&mut deps.storage, &admin).unwrap();

        // Helper to create BalanceKey
        let key = |user: &str| {
            BalanceKey {
                cross_chain_user: CrossChainUser::new(
                    ChainUid::vsl_chain_uid().unwrap(),
                    user.to_string(),
                ),
                token_id: "eucl".to_string(),
            }
            .to_serialized_balance_key()
        };

        // Helper to execute remove
        let remove_zeros = |deps: &mut cosmwasm_std::OwnedDeps<_, _, _>| {
            execute(
                deps.as_mut(),
                env.clone(),
                MessageInfo {
                    sender: admin.general_admin.clone(),
                    funds: vec![],
                },
                ExecuteMsg::RemoveZeroStateValues {
                    start_after: None,
                    limit: None,
                },
            )
            .unwrap();
        };

        // Save allowances
        ALLOWANCES
            .save(
                &mut deps.storage,
                key("spender"),
                &Allowance {
                    amount: Uint128::new(10),
                    spender: CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        "spender".to_string(),
                    ),
                },
            )
            .unwrap();

        ALLOWANCES
            .save(
                &mut deps.storage,
                key("spender2"),
                &Allowance {
                    amount: Uint128::zero(),
                    spender: CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        "spender2".to_string(),
                    ),
                },
            )
            .unwrap();

        // Assert initial state
        assert_eq!(
            ALLOWANCES
                .load(&deps.storage, key("spender"))
                .unwrap()
                .amount,
            Uint128::new(10)
        );
        assert_eq!(
            ALLOWANCES
                .load(&deps.storage, key("spender2"))
                .unwrap()
                .amount,
            Uint128::zero()
        );

        // Remove zero allowances
        remove_zeros(&mut deps);

        // Assert zero was removed, non-zero remains
        assert!(ALLOWANCES.load(&deps.storage, key("spender2")).is_err());
        assert!(ALLOWANCES.load(&deps.storage, key("spender")).is_ok());

        // Save balances
        BALANCES
            .save(&mut deps.storage, key("owner"), &Uint128::zero())
            .unwrap();
        BALANCES
            .save(&mut deps.storage, key("owner2"), &Uint128::new(10))
            .unwrap();

        // Remove zero balances
        remove_zeros(&mut deps);

        // Assert zero was removed, non-zero remains
        assert!(BALANCES.load(&deps.storage, key("owner")).is_err());
        assert!(BALANCES.load(&deps.storage, key("owner2")).is_ok());
    }

    mod cross_chain_user_test {
        use super::*;

        fn setup_with_state(
            deps: &mut cosmwasm_std::OwnedDeps<
                cosmwasm_std::MemoryStorage,
                cosmwasm_std::testing::MockApi,
                MockQuerier,
            >,
        ) -> Addr {
            let router = deps.api.addr_make("router");
            let admin = EuclidAdmin::default(router.clone());
            STATE
                .save(&mut deps.storage, &State { router: router.clone() })
                .unwrap();
            ADMIN.save(&mut deps.storage, &admin).unwrap();
            router
        }

        #[test]
        fn test_mint_rejects_mixed_case() {
            let mut deps = mock_dependencies();
            let env = mock_env();
            let router = setup_with_state(&mut deps);
            let info = MessageInfo {
                sender: router,
                funds: vec![],
            };

            let mixed_case_user = CrossChainUser::new(
                ChainUid::create("cosmos".to_string()).unwrap(),
                "Cosmos1AbCdEf".to_string(),
            );
            let balance_key = BalanceKey {
                cross_chain_user: mixed_case_user,
                token_id: "eucl".to_string(),
            };

            let msg = ExecuteMsg::Mint(ExecuteMint {
                amount: Uint128::new(100),
                balance_key,
            });

            let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
            assert!(err.to_string().contains("Address must be lowercase"));
        }

        #[test]
        fn test_transfer_rejects_mixed_case_to() {
            let mut deps = mock_dependencies();
            let env = mock_env();
            let router = setup_with_state(&mut deps);

            // First mint some tokens to a valid user
            let valid_user = CrossChainUser::new(
                ChainUid::create("cosmos".to_string()).unwrap(),
                "cosmos1sender".to_string(),
            );
            let balance_key = BalanceKey {
                cross_chain_user: valid_user.clone(),
                token_id: "eucl".to_string(),
            };
            let mint_msg = ExecuteMsg::Mint(ExecuteMint {
                amount: Uint128::new(100),
                balance_key,
            });
            let info = MessageInfo {
                sender: router.clone(),
                funds: vec![],
            };
            execute(deps.as_mut(), env.clone(), info, mint_msg).unwrap();

            // Try to transfer to a mixed-case address
            let mixed_case_to = CrossChainUser::new(
                ChainUid::create("cosmos".to_string()).unwrap(),
                "Cosmos1MiXeD".to_string(),
            );
            let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
                amount: Uint128::new(50),
                token_id: "eucl".to_string(),
                sender: Some(valid_user),
                to: mixed_case_to,
                from: None,
                msg: None,
            });
            let info = MessageInfo {
                sender: router,
                funds: vec![],
            };

            let err = execute(deps.as_mut(), env, info, transfer_msg).unwrap_err();
            assert!(err.to_string().contains("Address must be lowercase"));
        }

        #[test]
        fn test_approve_rejects_mixed_case_spender() {
            let mut deps = mock_dependencies();
            let env = mock_env();
            let router = setup_with_state(&mut deps);

            let owner = CrossChainUser::new(
                ChainUid::vsl_chain_uid().unwrap(),
                "owner".to_string(),
            );
            let mixed_case_spender = CrossChainUser::new(
                ChainUid::vsl_chain_uid().unwrap(),
                "Spender".to_string(),
            );

            let approve_msg = ExecuteMsg::Approve(ExecuteApprove {
                amount: Uint128::new(10),
                token_id: "eucl".to_string(),
                spender: mixed_case_spender,
                owner: owner,
            });
            let info = MessageInfo {
                sender: router,
                funds: vec![],
            };

            let err = execute(deps.as_mut(), env, info, approve_msg).unwrap_err();
            assert!(err.to_string().contains("Address must be lowercase"));
        }

        #[test]
        fn test_normalize_balance_keys_basic() {
            let mut deps = mock_dependencies();
            let router = setup_with_state(&mut deps);
            let env = mock_env();
            let admin = ADMIN.load(&deps.storage).unwrap();

            // Directly write a mixed-case balance entry
            let mixed_key: (ChainUid, String, String) = (
                ChainUid::create("cosmos".to_string()).unwrap(),
                "Cosmos1AbC".to_string(),
                "eucl".to_string(),
            );
            BALANCES
                .save(&mut deps.storage, mixed_key.clone(), &Uint128::new(100))
                .unwrap();

            let msg = ExecuteMsg::NormalizeBalanceKeys {
                start_after: None,
                limit: None,
            };
            let info = MessageInfo {
                sender: admin.general_admin,
                funds: vec![],
            };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(
                res.attributes
                    .iter()
                    .find(|a| a.key == "normalized_count")
                    .unwrap()
                    .value,
                "1"
            );

            // Mixed-case entry removed
            assert!(BALANCES.load(&deps.storage, mixed_key).is_err());
            // Lowercase entry exists with same balance
            let normalized_key: (ChainUid, String, String) = (
                ChainUid::create("cosmos".to_string()).unwrap(),
                "cosmos1abc".to_string(),
                "eucl".to_string(),
            );
            assert_eq!(
                BALANCES.load(&deps.storage, normalized_key).unwrap(),
                Uint128::new(100)
            );
        }

        #[test]
        fn test_normalize_balance_keys_combines_balances() {
            let mut deps = mock_dependencies();
            let _router = setup_with_state(&mut deps);
            let env = mock_env();
            let admin = ADMIN.load(&deps.storage).unwrap();

            let chain = ChainUid::create("cosmos".to_string()).unwrap();
            let mixed_key: (ChainUid, String, String) =
                (chain.clone(), "Cosmos1AbC".to_string(), "eucl".to_string());
            let lowercase_key: (ChainUid, String, String) =
                (chain, "cosmos1abc".to_string(), "eucl".to_string());

            BALANCES
                .save(&mut deps.storage, mixed_key.clone(), &Uint128::new(100))
                .unwrap();
            BALANCES
                .save(&mut deps.storage, lowercase_key.clone(), &Uint128::new(50))
                .unwrap();

            let msg = ExecuteMsg::NormalizeBalanceKeys {
                start_after: None,
                limit: None,
            };
            let info = MessageInfo {
                sender: admin.general_admin,
                funds: vec![],
            };
            execute(deps.as_mut(), env, info, msg).unwrap();

            assert!(BALANCES.load(&deps.storage, mixed_key).is_err());
            assert_eq!(
                BALANCES.load(&deps.storage, lowercase_key).unwrap(),
                Uint128::new(150)
            );
        }

        #[test]
        fn test_normalize_balance_keys_pagination() {
            let mut deps = mock_dependencies();
            setup_with_state(&mut deps);
            let env = mock_env();
            let admin = ADMIN.load(&deps.storage).unwrap();

            let chain = ChainUid::create("cosmos".to_string()).unwrap();
            let key1: (ChainUid, String, String) =
                (chain.clone(), "AAAA".to_string(), "eucl".to_string());
            let key2: (ChainUid, String, String) =
                (chain, "BBBB".to_string(), "eucl".to_string());

            BALANCES
                .save(&mut deps.storage, key1.clone(), &Uint128::new(10))
                .unwrap();
            BALANCES
                .save(&mut deps.storage, key2.clone(), &Uint128::new(20))
                .unwrap();

            // Process only 1 entry
            let msg = ExecuteMsg::NormalizeBalanceKeys {
                start_after: None,
                limit: Some(1),
            };
            let info = MessageInfo {
                sender: admin.general_admin.clone(),
                funds: vec![],
            };
            let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
            assert_eq!(
                res.attributes
                    .iter()
                    .find(|a| a.key == "normalized_count")
                    .unwrap()
                    .value,
                "1"
            );

            // key1 should be normalized, key2 still mixed
            assert!(BALANCES.load(&deps.storage, key1.clone()).is_err());
            assert!(BALANCES.load(&deps.storage, key2.clone()).is_ok());

            // Process remaining with start_after
            let msg = ExecuteMsg::NormalizeBalanceKeys {
                start_after: Some(key1),
                limit: Some(10),
            };
            let info = MessageInfo {
                sender: admin.general_admin,
                funds: vec![],
            };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(
                res.attributes
                    .iter()
                    .find(|a| a.key == "normalized_count")
                    .unwrap()
                    .value,
                "1"
            );
            assert!(BALANCES.load(&deps.storage, key2).is_err());
        }

        #[test]
        fn test_normalize_balance_keys_admin_gated() {
            let mut deps = mock_dependencies();
            setup_with_state(&mut deps);
            let env = mock_env();

            let not_admin = deps.api.addr_make("not_admin");
            let msg = ExecuteMsg::NormalizeBalanceKeys {
                start_after: None,
                limit: None,
            };
            let info = MessageInfo {
                sender: not_admin,
                funds: vec![],
            };
            let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
            assert_eq!(err, ContractError::Unauthorized {});
        }

        #[test]
        fn test_normalize_balance_keys_noop() {
            let mut deps = mock_dependencies();
            setup_with_state(&mut deps);
            let env = mock_env();
            let admin = ADMIN.load(&deps.storage).unwrap();

            let chain = ChainUid::create("cosmos".to_string()).unwrap();
            let key: (ChainUid, String, String) =
                (chain, "alreadylowercase".to_string(), "eucl".to_string());
            BALANCES
                .save(&mut deps.storage, key, &Uint128::new(100))
                .unwrap();

            let msg = ExecuteMsg::NormalizeBalanceKeys {
                start_after: None,
                limit: None,
            };
            let info = MessageInfo {
                sender: admin.general_admin,
                funds: vec![],
            };
            let res = execute(deps.as_mut(), env, info, msg).unwrap();
            assert_eq!(
                res.attributes
                    .iter()
                    .find(|a| a.key == "normalized_count")
                    .unwrap()
                    .value,
                "0"
            );
        }
    }
}
