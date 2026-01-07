#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate};
    use crate::state::{Allowance, ALLOWANCES, BALANCES, STATE};
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{Addr, MessageInfo, Response, Uint128};
    use euclid::chain::{ChainUid, CrossChainUser};
    use euclid::error::ContractError;
    use euclid::msgs::virtual_balance::{
        ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteMsg, ExecuteTransfer, InstantiateMsg,
        State,
    };
    use euclid::virtual_balance::BalanceKey;

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
            admin: admin.clone(),
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
        let admin = deps.api.addr_make("admin");

        let expected_state = State {
            router: router.to_string(),
            admin,
        };
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
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
        let router = Addr::unchecked("router");
        let admin = Addr::unchecked("admin");
        let state = State {
            router: router.to_string(),
            admin: admin.clone(),
        };
        STATE.save(&mut deps.storage, &state).unwrap();

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
    fn test_token_pause() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let router = Addr::unchecked("router");
        let admin = Addr::unchecked("admin");
        let state = State {
            router: router.to_string(),
            admin: admin.clone(),
        };
        STATE.save(&mut deps.storage, &state).unwrap();

        // Setup users
        let owner = CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "owner".to_string());
        let spender =
            CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "spender".to_string());
        let recipient = CrossChainUser::new(
            ChainUid::create("1".to_string()).unwrap(),
            "recipient".to_string(),
        );

        let balance_key = BalanceKey {
            cross_chain_user: owner.clone(),
            token_id: "eucl".to_string(),
        };

        let pause_msg = ExecuteMsg::PauseToken {
            chain_uid: ChainUid::vsl_chain_uid().unwrap(),
            token_id: "eucl".to_string(),
        };
        let info = MessageInfo {
            sender: admin.clone(),
            funds: vec![],
        };
        execute(deps.as_mut(), env.clone(), info.clone(), pause_msg).unwrap();

        // Mint token should fail
        let mint_msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint128::new(10),
            balance_key: balance_key.clone(),
        });
        let err = execute(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender: router.clone(),
                funds: vec![],
            },
            mint_msg,
        )
        .unwrap_err();
        assert_eq!(
            ContractError::TokenPaused {
                msg: "This token's operation is paused, withdrawal is available".to_string(),
            },
            err
        );

        // Transfer token should fail
        //TODO fix this case
        let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: Uint128::new(10),
            token_id: "eucl".to_string(),
            from: Some(owner.clone()),
            to: recipient.clone(),
            sender: None,
            msg: None,
        });
        let err = execute(deps.as_mut(), env.clone(), info.clone(), transfer_msg).unwrap_err();
        assert_eq!(
            ContractError::TokenPaused {
                msg: "This token's operation is paused, withdrawal is available".to_string(),
            },
            err
        );

        let approve_msg = ExecuteMsg::Approve(ExecuteApprove {
            amount: Uint128::new(10),
            token_id: "eucl".to_string(),
            spender: spender.clone(),
            owner: owner.clone(),
        });
        // Approvals are not affected by pause
        execute(
            deps.as_mut(),
            env.clone(),
            MessageInfo {
                sender: router.clone(),
                funds: vec![],
            },
            approve_msg,
        )
        .unwrap();
    }

    #[test]
    fn test_remove_zero_state_values() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let router = Addr::unchecked("router");
        let admin = Addr::unchecked("admin");

        // Save initial state
        STATE
            .save(
                &mut deps.storage,
                &State {
                    router: router.to_string(),
                    admin: admin.clone(),
                },
            )
            .unwrap();

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
                    sender: admin.clone(),
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
    #[test]
    fn test_paused_tokens_queries() {
        use cosmwasm_std::from_json;
        use euclid::msgs::virtual_balance::{
            GetAllPausedTokensResponse, GetPausedTokenHeightResponse, QueryMsg,
        };

        let mut deps = mock_dependencies();

        // Add a paused token entry
        let chain_uid1 = ChainUid::create("10".to_string()).unwrap();
        let token_id1 = "tokenA".to_string();
        let height1 = 42u64;
        crate::state::PAUSED_TOKENS
            .save(
                &mut deps.storage,
                (chain_uid1.clone(), token_id1.clone()),
                &height1,
            )
            .unwrap();

        // Add a second paused token entry
        let chain_uid2 = ChainUid::create("20".to_string()).unwrap();
        let token_id2 = "tokenB".to_string();
        let height2 = 77u64;
        crate::state::PAUSED_TOKENS
            .save(
                &mut deps.storage,
                (chain_uid2.clone(), token_id2.clone()),
                &height2,
            )
            .unwrap();

        // Test GetPausedTokenHeight query for first token
        let query_resp = crate::contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetPausedTokenHeight {
                chain_uid: chain_uid1.clone(),
                token_id: token_id1.clone(),
            },
        )
        .unwrap();
        let resp: GetPausedTokenHeightResponse = from_json(query_resp).unwrap();
        assert_eq!(resp.paused_token_height, height1);

        // Test GetPausedTokenHeight query for second token
        let query_resp = crate::contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetPausedTokenHeight {
                chain_uid: chain_uid2.clone(),
                token_id: token_id2.clone(),
            },
        )
        .unwrap();
        let resp: GetPausedTokenHeightResponse = from_json(query_resp).unwrap();
        assert_eq!(resp.paused_token_height, height2);

        // Test GetAllPausedTokens query returns both tokens
        let query_resp = crate::contract::query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetAllPausedTokens { pagination: None },
        )
        .unwrap();
        let all_paused: GetAllPausedTokensResponse = from_json(query_resp).unwrap();
        let mut expected = vec![
            (chain_uid1.clone(), token_id1.clone()),
            (chain_uid2.clone(), token_id2.clone()),
        ];
        let mut got = all_paused.paused_tokens.clone();
        // Sort for equality check since order is not guaranteed
        expected.sort_by(|a, b| a.0.cmp(&b.0));
        got.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(got, expected);
    }
}
