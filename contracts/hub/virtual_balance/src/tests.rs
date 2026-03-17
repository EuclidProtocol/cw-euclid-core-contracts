#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {

    use crate::contract::{execute, instantiate, query};
    use crate::normalize::{normalize, normalize_token_to_voucher, normalize_voucher_to_token};
    use crate::state::{
        ADMIN, VOUCHER_ALLOWANCES, VOUCHER_BALANCES, VoucherAllowance,
        get_escrow_balance_key,
    };

    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{Addr, Uint256, from_json};
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::error::ContractError;
    use euclid::msgs::virtual_balance::msg::{
        ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteMsg, ExecuteTransfer, GetBalanceResponse,
        InstantiateMsg, State,
    };
    use euclid::token::{Token, TokenMetadata, TokenType};
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

    fn register_metadata(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            MockQuerier,
        >,
        token_id: &str,
        chain_uid: ChainUid,
        token_type: TokenType,
        decimals: u8,
    ) {
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RegisterTokenMetadata {
            token_metadata: TokenMetadata {
                token: Token::create(token_id.to_string()).unwrap(),
                chain_uid,
                token_type,
                decimals,
                allowed: true,
            },
        };
        execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    }

    fn make_chain_uid(name: &str) -> ChainUid {
        ChainUid::create(name.to_string()).unwrap()
    }

    fn make_user(chain: &str, addr: &str) -> CrossChainUser {
        CrossChainUser::new(make_chain_uid(chain), addr.to_string())
    }

    fn native_token_type() -> TokenType {
        TokenType::Native {
            denom: "uatom".to_string(),
        }
    }

    // ======== Normalization Tests ========

    #[test]
    fn test_normalize_same_decimals() {
        let amount = Uint256::from(1_000_000u128);
        let result = normalize(amount, 6, 6).unwrap();
        assert_eq!(result, amount);
    }

    #[test]
    fn test_normalize_scale_up() {
        // 6 decimals -> 24 decimals: multiply by 10^18
        let amount = Uint256::from(1_000_000u128); // 1.0 with 6 decimals
        let result = normalize(amount, 6, 24).unwrap();
        let expected = Uint256::from(1_000_000u128) * Uint256::from(10u128).pow(18);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_normalize_scale_down() {
        // 24 decimals -> 6 decimals: divide by 10^18
        let amount = Uint256::from(10u128).pow(24); // 1.0 with 24 decimals
        let result = normalize(amount, 24, 6).unwrap();
        assert_eq!(result, Uint256::from(1_000_000u128));
    }

    #[test]
    fn test_normalize_round_trip() {
        // Normalize then denormalize should return original for exact amounts
        let original = Uint256::from(1_000_000u128);
        let metadata = TokenMetadata {
            token: Token::create("token1".to_string()).unwrap(),
            chain_uid: make_chain_uid("chain1"),
            token_type: native_token_type(),
            decimals: 6,
            allowed: true,
        };
        let normalized = normalize_token_to_voucher(original, metadata.clone()).unwrap();
        let denormalized = normalize_voucher_to_token(normalized, metadata).unwrap();
        assert_eq!(denormalized, original);
    }

    #[test]
    fn test_normalize_zero_amount() {
        let result = normalize(Uint256::zero(), 6, 24).unwrap();
        assert_eq!(result, Uint256::zero());
    }

    #[test]
    fn test_normalize_18_to_24_decimals() {
        let amount = Uint256::from(10u128).pow(18); // 1.0 with 18 decimals
        let result = normalize(amount, 18, 24).unwrap();
        let expected = Uint256::from(10u128).pow(24);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_normalize_24_decimals_identity() {
        let amount = Uint256::from(10u128).pow(24);
        let metadata = TokenMetadata {
            token: Token::create("token1".to_string()).unwrap(),
            chain_uid: make_chain_uid("chain1"),
            token_type: native_token_type(),
            decimals: 24,
            allowed: true,
        };
        let result = normalize_token_to_voucher(amount, metadata).unwrap();
        assert_eq!(result, amount);
    }

    // ======== Mint Tests ========

    #[test]
    fn test_mint_normalized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        // Register token metadata with 6 decimals
        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let user = make_user("chain1", "user1");
        let balance_key = BalanceKey {
            cross_chain_user: user.clone(),
            token_id: "token1".to_string(),
        };

        // Mint 1_000_000 raw tokens (1.0 with 6 decimals)
        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(1_000_000u128),
            balance_key: balance_key.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });

        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Verify balance is normalized to 24 decimals
        let key = balance_key.clone().to_serialized_balance_key();
        let balance = VOUCHER_BALANCES.load(&deps.storage, key).unwrap();
        let expected_normalized =
            Uint256::from(1_000_000u128) * Uint256::from(10u128).pow(18);
        assert_eq!(balance, expected_normalized);

        // Verify escrow balance stores raw amount
        let escrow_key = get_escrow_balance_key(
            "token1".to_string(),
            chain.clone(),
            token_type.clone(),
        );
        let escrow = escrow_key.load(&deps.storage).unwrap();
        assert_eq!(escrow, Uint256::from(1_000_000u128));
    }

    #[test]
    fn test_mint_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);

        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(10u128),
            balance_key: BalanceKey {
                cross_chain_user: make_user("chain1", "user1"),
                token_id: "token1".to_string(),
            },
            token_type: native_token_type(),
            token_source_chain_uid: make_chain_uid("chain1"),
        });

        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_mint_zero_amount_rejected() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::zero(),
            balance_key: BalanceKey {
                cross_chain_user: make_user("chain1", "user1"),
                token_id: "token1".to_string(),
            },
            token_type: native_token_type(),
            token_source_chain_uid: make_chain_uid("chain1"),
        });

        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    // ======== Burn Tests ========

    #[test]
    fn test_burn_normalized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let user = make_user("chain1", "user1");
        let balance_key = BalanceKey {
            cross_chain_user: user.clone(),
            token_id: "token1".to_string(),
        };

        // Mint first
        let mint_msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(2_000_000u128),
            balance_key: balance_key.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info.clone(), mint_msg).unwrap();

        // Burn half (normalized amount)
        let normalized_half =
            Uint256::from(1_000_000u128) * Uint256::from(10u128).pow(18);
        let burn_msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: normalized_half,
            balance_key: balance_key.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info, burn_msg).unwrap();

        // Verify balance is half
        let key = balance_key.clone().to_serialized_balance_key();
        let balance = VOUCHER_BALANCES.load(&deps.storage, key).unwrap();
        assert_eq!(balance, normalized_half);

        // Verify escrow is decremented
        let escrow_key = get_escrow_balance_key(
            "token1".to_string(),
            chain.clone(),
            token_type.clone(),
        );
        let escrow = escrow_key.load(&deps.storage).unwrap();
        assert_eq!(escrow, Uint256::from(1_000_000u128));
    }

    #[test]
    fn test_burn_clears_zero_balance() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let balance_key = BalanceKey {
            cross_chain_user: make_user("chain1", "user1"),
            token_id: "token1".to_string(),
        };

        // Mint
        let mint_msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(1_000_000u128),
            balance_key: balance_key.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info.clone(), mint_msg).unwrap();

        // Burn all (normalized)
        let key = balance_key.clone().to_serialized_balance_key();
        let full_balance = VOUCHER_BALANCES.load(&deps.storage, key.clone()).unwrap();

        let burn_msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: full_balance,
            balance_key: balance_key.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info, burn_msg).unwrap();

        // Balance should be removed from storage
        assert!(VOUCHER_BALANCES.may_load(&deps.storage, key).unwrap().is_none());

        // Escrow should be removed
        let escrow_key =
            get_escrow_balance_key("token1".to_string(), chain.clone(), token_type.clone());
        assert!(escrow_key.may_load(&deps.storage).unwrap().is_none());
    }

    #[test]
    fn test_burn_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);

        let msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: Uint256::from(10u128),
            balance_key: BalanceKey {
                cross_chain_user: make_user("chain1", "user1"),
                token_id: "token1".to_string(),
            },
            token_type: native_token_type(),
            token_source_chain_uid: make_chain_uid("chain1"),
        });

        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    // ======== Transfer Tests ========

    #[test]
    fn test_transfer_uint256() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let user1 = make_user("chain1", "user1");
        let user2 = make_user("chain1", "user2");

        let balance_key_1 = BalanceKey {
            cross_chain_user: user1.clone(),
            token_id: "token1".to_string(),
        };

        // Mint to user1
        let mint_msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(10_000_000u128),
            balance_key: balance_key_1.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info.clone(), mint_msg).unwrap();

        // Get normalized balance
        let key1 = balance_key_1.clone().to_serialized_balance_key();
        let user1_balance = VOUCHER_BALANCES.load(&deps.storage, key1.clone()).unwrap();

        // Transfer half
        let transfer_amount = user1_balance / Uint256::from(2u128);
        let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: transfer_amount,
            token_id: "token1".to_string(),
            sender: Some(user1.clone()),
            to: user2.clone(),
            from: None,
            msg: None,
        });
        execute(deps.as_mut(), env.clone(), info, transfer_msg).unwrap();

        // Verify balances
        let balance_key_2 = BalanceKey {
            cross_chain_user: user2.clone(),
            token_id: "token1".to_string(),
        };
        let key2 = balance_key_2.clone().to_serialized_balance_key();

        let user1_after = VOUCHER_BALANCES.load(&deps.storage, key1).unwrap();
        let user2_after = VOUCHER_BALANCES.load(&deps.storage, key2).unwrap();

        assert_eq!(user1_after, user1_balance - transfer_amount);
        assert_eq!(user2_after, transfer_amount);
    }

    #[test]
    fn test_transfer_zero_rejected() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: Uint256::zero(),
            token_id: "token1".to_string(),
            sender: Some(make_user("chain1", "user1")),
            to: make_user("chain1", "user2"),
            from: None,
            msg: None,
        });

        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    // ======== Allowance Tests ========

    #[test]
    fn test_approve_and_allowance_transfer() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "eucl", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let router_info = message_info(&router, &[]);

        let owner = make_user("chain1", "owner");
        let spender = make_user("chain1", "spender");
        let recipient = make_user("chain1", "recipient");

        let balance_key = BalanceKey {
            cross_chain_user: owner.clone(),
            token_id: "eucl".to_string(),
        };

        // Mint to owner
        let mint_msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(20_000_000u128),
            balance_key: balance_key.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), router_info.clone(), mint_msg).unwrap();

        // Get normalized balance for approval
        let key = balance_key.clone().to_serialized_balance_key();
        let owner_balance = VOUCHER_BALANCES.load(&deps.storage, key.clone()).unwrap();

        // Approve spender for half the balance
        let approve_amount = owner_balance / Uint256::from(2u128);
        let approve_msg = ExecuteMsg::Approve(ExecuteApprove {
            amount: approve_amount,
            token_id: "eucl".to_string(),
            spender: spender.clone(),
            owner: owner.clone(),
        });
        execute(deps.as_mut(), env.clone(), router_info.clone(), approve_msg).unwrap();

        // Verify allowance was set
        let allowance = VOUCHER_ALLOWANCES
            .load(&deps.storage, key.clone())
            .unwrap();
        assert_eq!(allowance.amount, approve_amount);
        assert_eq!(allowance.spender, spender);

        // Spender transfers to recipient via router
        let transfer_amount = approve_amount / Uint256::from(2u128);
        let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: transfer_amount,
            token_id: "eucl".to_string(),
            from: Some(owner.clone()),
            to: recipient.clone(),
            sender: Some(spender.clone()),
            msg: None,
        });
        execute(deps.as_mut(), env.clone(), router_info.clone(), transfer_msg).unwrap();

        // Verify allowance deducted
        let allowance = VOUCHER_ALLOWANCES
            .load(&deps.storage, key.clone())
            .unwrap();
        assert_eq!(allowance.amount, approve_amount - transfer_amount);

        // Transfer remaining allowance
        let remaining = approve_amount - transfer_amount;
        let transfer_msg = ExecuteMsg::Transfer(ExecuteTransfer {
            amount: remaining,
            token_id: "eucl".to_string(),
            from: Some(owner.clone()),
            to: recipient.clone(),
            sender: Some(spender.clone()),
            msg: None,
        });
        execute(deps.as_mut(), env.clone(), router_info, transfer_msg).unwrap();

        // Allowance should be removed
        assert!(VOUCHER_ALLOWANCES.load(&deps.storage, key).is_err());
    }

    // ======== Token Metadata Tests ========

    #[test]
    fn test_register_token_metadata() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        // Attempt duplicate registration
        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RegisterTokenMetadata {
            token_metadata: TokenMetadata {
                token: Token::create("token1".to_string()).unwrap(),
                chain_uid: chain.clone(),
                token_type: token_type.clone(),
                decimals: 6,
                allowed: true,
            },
        };
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(format!("{err:?}").contains("already registered"));
    }

    #[test]
    fn test_register_token_metadata_invalid_decimals() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);
        let msg = ExecuteMsg::RegisterTokenMetadata {
            token_metadata: TokenMetadata {
                token: Token::create("token1".to_string()).unwrap(),
                chain_uid: make_chain_uid("chain1"),
                token_type: native_token_type(),
                decimals: 25, // > VOUCHER_DECIMAL (24)
                allowed: true,
            },
        };
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidDecimals { .. }));
    }

    #[test]
    fn test_update_token_metadata_allowed_field() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        // Update allowed field (admin only)
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(router.clone());
        let admin_info = message_info(&admin.general_admin, &[]);

        let msg = ExecuteMsg::UpdateTokenMetadata {
            token_metadata: TokenMetadata {
                token: Token::create("token1".to_string()).unwrap(),
                chain_uid: chain.clone(),
                token_type: token_type.clone(),
                decimals: 6,
                allowed: false, // Changed
            },
        };
        execute(deps.as_mut(), mock_env(), admin_info, msg).unwrap();
    }

    #[test]
    fn test_update_token_metadata_cannot_change_decimals() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(router.clone());
        let admin_info = message_info(&admin.general_admin, &[]);

        let msg = ExecuteMsg::UpdateTokenMetadata {
            token_metadata: TokenMetadata {
                token: Token::create("token1".to_string()).unwrap(),
                chain_uid: chain.clone(),
                token_type: token_type.clone(),
                decimals: 18, // Attempt to change from 6 to 18
                allowed: true,
            },
        };
        let err = execute(deps.as_mut(), mock_env(), admin_info, msg).unwrap_err();
        assert!(format!("{err:?}").contains("Cannot change token decimals"));
    }

    // ======== Escrow Tracking Tests ========

    #[test]
    fn test_escrow_tracking_multiple_operations() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let user1 = make_user("chain1", "user1");
        let user2 = make_user("chain1", "user2");

        // Mint 5M to user1
        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(5_000_000u128),
            balance_key: BalanceKey {
                cross_chain_user: user1.clone(),
                token_id: "token1".to_string(),
            },
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // Mint 3M to user2
        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(3_000_000u128),
            balance_key: BalanceKey {
                cross_chain_user: user2.clone(),
                token_id: "token1".to_string(),
            },
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // Escrow should be 8M raw
        let escrow_key = get_escrow_balance_key(
            "token1".to_string(),
            chain.clone(),
            token_type.clone(),
        );
        let escrow = escrow_key.load(&deps.storage).unwrap();
        assert_eq!(escrow, Uint256::from(8_000_000u128));

        // Burn 2M normalized from user1
        let normalized_2m =
            Uint256::from(2_000_000u128) * Uint256::from(10u128).pow(18);
        let msg = ExecuteMsg::Burn(ExecuteBurn {
            amount: normalized_2m,
            balance_key: BalanceKey {
                cross_chain_user: user1.clone(),
                token_id: "token1".to_string(),
            },
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Escrow should be 6M raw
        let escrow = escrow_key.load(&deps.storage).unwrap();
        assert_eq!(escrow, Uint256::from(6_000_000u128));
    }

    // ======== Remove Zero State Values Tests ========

    #[test]
    fn test_remove_zero_state_values() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(router.clone());

        // Save initial state
        crate::state::STATE
            .save(
                &mut deps.storage,
                &State {
                    router: router.clone(),
                },
            )
            .unwrap();
        ADMIN.save(&mut deps.storage, &admin).unwrap();

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

        // Save allowances
        VOUCHER_ALLOWANCES
            .save(
                &mut deps.storage,
                key("spender"),
                &VoucherAllowance {
                    amount: Uint256::from(10u128),
                    spender: CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        "spender".to_string(),
                    ),
                    expires_at: None,
                },
            )
            .unwrap();

        VOUCHER_ALLOWANCES
            .save(
                &mut deps.storage,
                key("spender2"),
                &VoucherAllowance {
                    amount: Uint256::zero(),
                    spender: CrossChainUser::new(
                        ChainUid::vsl_chain_uid().unwrap(),
                        "spender2".to_string(),
                    ),
                    expires_at: None,
                },
            )
            .unwrap();

        // Save balances
        VOUCHER_BALANCES
            .save(&mut deps.storage, key("owner"), &Uint256::zero())
            .unwrap();
        VOUCHER_BALANCES
            .save(&mut deps.storage, key("owner2"), &Uint256::from(10u128))
            .unwrap();

        // Execute cleanup
        let remove_msg = ExecuteMsg::RemoveZeroStateValues {
            start_after: None,
            limit: None,
        };
        let info = MessageInfo {
            sender: admin.general_admin.clone(),
            funds: vec![],
        };
        execute(deps.as_mut(), env, info, remove_msg).unwrap();

        // Zero allowance removed, non-zero remains
        assert!(VOUCHER_ALLOWANCES
            .load(&deps.storage, key("spender2"))
            .is_err());
        assert!(VOUCHER_ALLOWANCES
            .load(&deps.storage, key("spender"))
            .is_ok());

        // Zero balance removed, non-zero remains
        assert!(VOUCHER_BALANCES
            .load(&deps.storage, key("owner"))
            .is_err());
        assert!(VOUCHER_BALANCES
            .load(&deps.storage, key("owner2"))
            .is_ok());
    }

    // ======== Query Tests ========

    #[test]
    fn test_query_balance() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let chain = make_chain_uid("chain1");
        let token_type = native_token_type();

        register_metadata(&mut deps, "token1", chain.clone(), token_type.clone(), 6);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        let user = make_user("chain1", "user1");
        let balance_key = BalanceKey {
            cross_chain_user: user.clone(),
            token_id: "token1".to_string(),
        };

        // Mint
        let msg = ExecuteMsg::Mint(ExecuteMint {
            amount: Uint256::from(1_000_000u128),
            balance_key: balance_key.clone(),
            token_type: token_type.clone(),
            token_source_chain_uid: chain.clone(),
        });
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Query balance
        let query_msg =
            euclid::msgs::virtual_balance::msg::QueryMsg::GetBalance { balance_key };
        let res = query(deps.as_ref(), env, query_msg).unwrap();
        let balance_res: GetBalanceResponse = from_json(res).unwrap();

        let expected =
            Uint256::from(1_000_000u128) * Uint256::from(10u128).pow(18);
        assert_eq!(balance_res.amount, expected);
    }

    // ======== Init Test ========

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);
        assert_eq!(0, res.messages.len());
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(router.clone());

        let expected_state = State { router };
        let state = crate::state::STATE.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
        let saved_admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved_admin, admin);
    }
}
