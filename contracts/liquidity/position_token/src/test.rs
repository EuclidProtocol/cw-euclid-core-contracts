#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{from_json, Addr, Int256, OverflowError, OverflowOperation, Uint128};
    use euclid::msgs::factory;

    use crate::contract::{execute, instantiate, query};
    use euclid::error::ContractError;
    use euclid::msgs::position_token::{
        ExecuteMsg, InstantiateMsg, MintMsg, OwnerOfResponse, PositionInfo, PositionInfoResponse,
        QueryMsg, StateResponse, TokenInfo, TokensResponse,
    };
    use euclid::utils::pagination::Pagination;
    use rstest::rstest;

    type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
        cosmwasm_std::testing::MockApi,
        cosmwasm_std::testing::MockQuerier,
    >;

    /// When `expected_error` is `None`, requires `Ok` and returns the inner value. When `Some`,
    /// requires `Err` matching exactly (including [`ContractError::NotFound`] message text).
    fn assert_result<T>(
        result: Result<T, ContractError>,
        expected_error: Option<ContractError>,
    ) -> Option<T> {
        match (expected_error, result) {
            (None, Ok(value)) => Some(value),
            (None, Err(err)) => panic!("expected success, got {err:?}"),
            (Some(expected), Ok(_)) => panic!("expected error {expected:?}, got Ok"),
            (Some(expected), Err(actual)) => {
                assert_eq!(actual, expected);
                None
            }
        }
    }

    fn setup() -> (MockDeps, Addr) {
        let mut deps = mock_dependencies();

        let factory = deps.api.addr_make("factory");

        assert_result(
            instantiate(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                InstantiateMsg {
                    name: "Position Token".to_string(),
                    symbol: "POS".to_string(),
                    vlp_address: "vlp-1".to_string(),
                    mint_msg: None,
                },
            ),
            None,
        )
        .unwrap();

        (deps, factory)
    }

    #[rstest]
    #[case("Position Token", "POS", "vlp-1", None)]
    #[case("Pool NFT", "PNFT", "vlp-custom", None)]
    #[case("", "POS", "vlp-1", Some(ContractError::Generic {
        err: "name cannot be empty".to_string(),
    }))]
    #[case("   ", "POS", "vlp-1", Some(ContractError::Generic {
        err: "name cannot be empty".to_string(),
    }))]
    #[case("Name", "", "vlp-1", Some(ContractError::Generic {
        err: "symbol cannot be empty".to_string(),
    }))]
    fn instantiate_respects_metadata(
        #[case] name: &str,
        #[case] symbol: &str,
        #[case] vlp_address: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        let mut deps = mock_dependencies();
        let factory = deps.api.addr_make("factory");

        let outcome = assert_result(
            instantiate(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                InstantiateMsg {
                    name: name.to_string(),
                    symbol: symbol.to_string(),
                    vlp_address: vlp_address.to_string(),
                    mint_msg: None,
                },
            ),
            expected_error,
        );

        if outcome.is_none() {
            return;
        }

        let state: StateResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap()).unwrap();

        assert_eq!(state.name, name);
        assert_eq!(state.symbol, symbol);
        assert_eq!(state.factory, factory);
        assert_eq!(state.vlp_address, vlp_address);
        assert_eq!(state.total_tokens, 0);
    }

    #[rstest]
    #[case("factory", None)]
    #[case("stranger", Some(ContractError::Unauthorized {}))]
    fn mint_respects_factory_only(
        #[case] sender: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        let (mut deps, _) = setup();
        let owner = deps.api.addr_make("owner");
        let sender = deps.api.addr_make(sender);

        let result = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::Mint(MintMsg {
                token_id: "position-1".to_string(),
                token_info: TokenInfo {
                    owner: owner.clone(),
                    token_uri: Some("ipfs://position-1".to_string()),
                },
                position_info: PositionInfo {
                    liquidity: Uint128::new(1000),
                },
            }),
        );

        if assert_result(result, expected_error).is_none() {
            return;
        }

        let owner_of: OwnerOfResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::OwnerOf {
                    token_id: "position-1".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(owner_of.owner, owner.to_string());

        let position: PositionInfoResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::PositionInfo {
                    token_id: "position-1".to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(position.liquidity, Uint128::new(1000));

        let state: StateResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap()).unwrap();
        assert_eq!(state.total_tokens, 1);
    }

    #[rstest]
    #[case::valid_with_uri("position-1", Some("ipfs://position-1"), None)]
    #[case::valid_without_uri("pos-alt", None, None)]
    #[case::invalid_token_id("", None, Some(ContractError::InvalidTokenID {}))]
    fn mint_tests(
        #[case] token_id: &str,
        #[case] token_uri: Option<&str>,
        #[case] expected_error: Option<ContractError>,
    ) {
        let (mut deps, factory) = setup();
        let owner = deps.api.addr_make("owner");

        let result = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&factory, &[]),
            ExecuteMsg::Mint(MintMsg {
                token_id: token_id.to_string(),
                token_info: TokenInfo {
                    owner: owner.clone(),
                    token_uri: token_uri.map(String::from),
                },
                position_info: PositionInfo {
                    liquidity: Uint128::new(1000),
                },
            }),
        );
        if assert_result(result, expected_error).is_none() {
            return;
        }

        let owner_of: OwnerOfResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::OwnerOf {
                    token_id: token_id.to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(owner_of.owner, owner.to_string());

        let position: PositionInfoResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::PositionInfo {
                    token_id: token_id.to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(position.liquidity, Uint128::new(1000));

        let state: StateResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap()).unwrap();
        assert_eq!(state.total_tokens, 1);
    }

    #[rstest]
    #[case::duplicate("dup-id", Some(ContractError::TokenAlreadyExist {}))]
    #[case::unique("unique-id", None)]
    fn mint_rejects_duplicate_token_id(
        #[case] token_id: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        let (mut deps, factory) = setup();
        let owner = deps.api.addr_make("owner");
        let mut mint = |token_id: &str| {
            execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                ExecuteMsg::Mint(MintMsg {
                    token_id: token_id.to_string(),
                    token_info: TokenInfo {
                        owner: owner.clone(),
                        token_uri: None,
                    },
                    position_info: PositionInfo {
                        liquidity: Uint128::new(1),
                    },
                }),
            )
        };
        assert_result(mint("dup-id"), None).unwrap();
        assert_result(mint(token_id), expected_error);
    }

    #[rstest]
    #[case::stranger("stranger", Int256::from(100i128), 1100u128, Some(ContractError::Unauthorized {}))]
    #[case::valid_positive("factory", Int256::from(100i128), 1100u128, None)]
    #[case::valid_negative("factory", Int256::from(-100i128), 900u128, None)]
    #[case::valid_zero("factory", Int256::zero(), 1000u128, None)]
    #[case::overflow_positive("factory", Int256::from(u128::MAX) + Int256::from(1), u128::MAX, Some(ContractError::new("liquidity change overflow")))]
    #[case::overflow_negative("factory", -Int256::from(u128::MAX) - Int256::from(1), 0u128, Some(ContractError::new("liquidity change overflow")))]
    fn update_position_changes_stored_liquidity(
        #[case] sender: &str,
        #[case] liquidity_change: Int256,
        #[case] expected_liquidity: u128,
        #[case] expected_error: Option<ContractError>,
    ) {
        let (mut deps, factory) = setup();
        let owner = deps.api.addr_make("owner");
        let token_id = "position-1".to_string();
        let sender = deps.api.addr_make(sender);

        assert_result(
            execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                ExecuteMsg::Mint(MintMsg {
                    token_id: token_id.clone(),
                    token_info: TokenInfo {
                        owner: owner,
                        token_uri: None,
                    },
                    position_info: PositionInfo {
                        liquidity: Uint128::new(1000),
                    },
                }),
            ),
            None,
        )
        .unwrap();

        let result = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&sender, &[]),
            ExecuteMsg::UpdatePosition {
                token_id: token_id.clone(),
                liquidity_change,
            },
        );

        if assert_result(result, expected_error).is_none() {
            return;
        }

        let position: PositionInfoResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::PositionInfo { token_id },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(position.liquidity, Uint128::new(expected_liquidity));
    }

    #[rstest]
    #[case(Some(ContractError::NotFound {
        msg: "position missing-token not found".to_string(),
    }))]
    fn update_position_rejects_unknown_token(#[case] expected_error: Option<ContractError>) {
        let (mut deps, factory) = setup();

        let result = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&factory, &[]),
            ExecuteMsg::UpdatePosition {
                token_id: "missing-token".to_string(),
                liquidity_change: Int256::zero(),
            },
        );
        assert_result(result, expected_error);
    }

    #[rstest]
    #[case("recipient_a", None)]
    #[case("recipient_b", None)]
    #[case("recipient_c", Some(ContractError::Unauthorized {}))]
    #[case("recipient_d", Some(ContractError::SameAddress {}))]
    fn transfer_respects_owner_and_recipient_rules(
        #[case] recipient_key: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        let (mut deps, factory) = setup();
        let owner = deps.api.addr_make("owner");
        let recipient = deps.api.addr_make(recipient_key);
        let token_id = "position-1".to_string();

        assert_result(
            execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                ExecuteMsg::Mint(MintMsg {
                    token_id: token_id.clone(),
                    token_info: TokenInfo {
                        owner: owner.clone(),
                        token_uri: None,
                    },
                    position_info: PositionInfo {
                        liquidity: Uint128::new(1000),
                    },
                }),
            ),
            None,
        )
        .unwrap();

        let transfer_msg = ExecuteMsg::Transfer {
            token_id: token_id.clone(),
            recipient: recipient.to_string(),
        };

        let transfer_result = match expected_error {
            Some(ContractError::Unauthorized {}) => execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                transfer_msg,
            ),
            Some(ContractError::SameAddress {}) => execute(
                deps.as_mut(),
                mock_env(),
                message_info(&owner, &[]),
                ExecuteMsg::Transfer {
                    token_id: token_id.clone(),
                    recipient: owner.to_string(),
                },
            ),
            _ => execute(
                deps.as_mut(),
                mock_env(),
                message_info(&owner, &[]),
                transfer_msg,
            ),
        };

        if assert_result(transfer_result, expected_error).is_none() {
            return;
        }

        let owner_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: owner.to_string(),
                    pagination: Pagination::default(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(owner_tokens.tokens.is_empty());

        let recipient_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: recipient.to_string(),
                    pagination: Pagination::default(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(recipient_tokens.tokens, vec![token_id]);
    }

    #[rstest]
    #[case("burn-a", None)]
    #[case("burn-b", None)]
    #[case("burn-c", Some(ContractError::Unauthorized {}))]
    #[case(
        "burn-d",
        Some(ContractError::Generic {
            err: "Cannot burn a position with liquidity".to_string(),
        })
    )]
    fn burn_requires_factory_and_zero_liquidity(
        #[case] token_id: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        let (mut deps, factory) = setup();
        let owner = deps.api.addr_make("owner");

        assert_result(
            execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                ExecuteMsg::Mint(MintMsg {
                    token_id: token_id.to_string(),
                    token_info: TokenInfo {
                        owner: owner.clone(),
                        token_uri: None,
                    },
                    position_info: PositionInfo {
                        liquidity: Uint128::new(1000),
                    },
                }),
            ),
            None,
        )
        .unwrap();

        if expected_error.is_none() {
            assert_result(
                execute(
                    deps.as_mut(),
                    mock_env(),
                    message_info(&factory, &[]),
                    ExecuteMsg::UpdatePosition {
                        token_id: token_id.to_string(),
                        liquidity_change: Int256::from(-1000i128),
                    },
                ),
                None,
            )
            .unwrap();
        }

        let burn_result = match expected_error {
            Some(ContractError::Unauthorized {}) => execute(
                deps.as_mut(),
                mock_env(),
                message_info(&owner, &[]),
                ExecuteMsg::Burn {
                    token_id: token_id.to_string(),
                },
            ),
            _ => execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                ExecuteMsg::Burn {
                    token_id: token_id.to_string(),
                },
            ),
        };

        if assert_result(burn_result, expected_error).is_none() {
            return;
        }

        let all_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::AllTokens {
                    pagination: Pagination::default(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(all_tokens.tokens.is_empty());

        let state: StateResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap()).unwrap();
        assert_eq!(state.total_tokens, 0);
    }

    #[rstest]
    #[case("w", None)]
    #[case("z", None)]
    fn transfer_then_burn_updates_owner_and_global_lists(
        #[case] prefix: &str,
        #[case] expected_error: Option<ContractError>,
    ) {
        let (mut deps, factory) = setup();
        let owner = deps.api.addr_make("owner");
        let recipient = deps.api.addr_make("recipient");

        let id = |n: u8| format!("{prefix}-{n}");
        for n in 1u8..=3 {
            assert_result(
                execute(
                    deps.as_mut(),
                    mock_env(),
                    message_info(&factory, &[]),
                    ExecuteMsg::Mint(MintMsg {
                        token_id: id(n),
                        token_info: TokenInfo {
                            owner: owner.clone(),
                            token_uri: None,
                        },
                        position_info: PositionInfo {
                            liquidity: Uint128::new(1000),
                        },
                    }),
                ),
                None,
            )
            .unwrap();
        }

        let owner_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: owner.to_string(),
                    pagination: Pagination::default(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(owner_tokens.tokens, vec![id(1), id(2), id(3)]);

        let result = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&owner, &[]),
            ExecuteMsg::Transfer {
                token_id: id(2),
                recipient: recipient.to_string(),
            },
        );
        if assert_result(result, expected_error).is_none() {
            return;
        }

        assert_result(
            execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                ExecuteMsg::UpdatePosition {
                    token_id: id(1),
                    liquidity_change: Int256::from(-1000i128),
                },
            ),
            None,
        )
        .unwrap();

        assert_result(
            execute(
                deps.as_mut(),
                mock_env(),
                message_info(&factory, &[]),
                ExecuteMsg::Burn { token_id: id(1) },
            ),
            None,
        )
        .unwrap();

        let owner_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: owner.to_string(),
                    pagination: Pagination::default(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(owner_tokens.tokens, vec![id(3)]);

        let recipient_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TokensByOwner {
                    owner: recipient.to_string(),
                    pagination: Pagination::default(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(recipient_tokens.tokens, vec![id(2)]);

        let all_tokens: TokensResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::AllTokens {
                    pagination: Pagination::default(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(all_tokens.tokens, vec![id(2), id(3)]);

        let state: StateResponse =
            from_json(query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap()).unwrap();
        assert_eq!(state.total_tokens, 2);
    }
}
