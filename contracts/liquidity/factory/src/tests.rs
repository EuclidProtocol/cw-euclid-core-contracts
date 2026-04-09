#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use euclid::error::ContractError;
    use rstest::rstest;

    use crate::contract::instantiate;
    use crate::execute::pool::{
        collect_concentrated_fees_request, remove_concentrated_liquidity_request,
    };
    use crate::state::{State, ADMIN, POOL_KEY_TO_VLP, STATE, VLP_TO_POSITION_TOKEN};

    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockApi, MockQuerier};
    use cosmwasm_std::{
        from_json, to_json_binary, Addr, Binary, ContractResult, DepsMut, MemoryStorage, OwnedDeps,
        Response, SystemResult, Uint128, WasmQuery,
    };
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::msgs;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::InstantiateMsg;
    use euclid::msgs::position_token::{OwnerOfResponse, PositionInfoResponse, TokenInfoResponse};
    use euclid::msgs::vlp::base::{PoolKey, PoolType};
    use euclid::token::{Pair, Token};

    // ── shared helpers ────────────────────────────────────────────────────────

    fn mock_pool_key() -> PoolKey {
        PoolKey {
            pair: Pair {
                token_1: Token::create("tokena".to_string()).unwrap(),
                token_2: Token::create("tokenb".to_string()).unwrap(),
            },
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
        }
    }

    fn mock_wrong_pool_key() -> PoolKey {
        PoolKey {
            pair: Pair {
                token_1: Token::create("wronga".to_string()).unwrap(),
                token_2: Token::create("wrongb".to_string()).unwrap(),
            },
            pool_type: PoolType::Concentrated {
                fee_tier_bps: 500,
                tick_spacing: 10,
            },
        }
    }

    /// Create a mock position token contract and setup the state for a concentrated pool.
    /// Add querier to return the expected responses for the position token contract.
    /// Setup state with postion id = 1 and liquidity = 1000 for the given pool key and nft owner.
    fn setup_concentrated_state(
        deps: &mut OwnedDeps<MemoryStorage, MockApi, MockQuerier>,
        nft_owner: Addr,
        pool_key: &PoolKey,
    ) {
        let position_token_address = deps.api.addr_make("position_nft");
        let position_token_address_str = position_token_address.to_string();

        let token_not_found_error = |token_id: String| -> SystemResult<ContractResult<Binary>> {
            SystemResult::Ok(ContractResult::Err(
                ContractError::NotFound {
                    msg: format!("token {token_id} not found"),
                }
                .to_string(),
            ))
        };
        deps.querier.update_wasm(move |query| match query {
            WasmQuery::Smart { contract_addr, msg } => {
                if contract_addr.to_string() != position_token_address_str {
                    return SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest {
                        kind: "invalid_contract_address".into(),
                    });
                }
                let parsed_msg = from_json::<msgs::position_token::QueryMsg>(&msg).unwrap();
                match parsed_msg {
                    msgs::position_token::QueryMsg::OwnerOf { token_id } => {
                        if token_id != "1".to_string() {
                            return token_not_found_error(token_id);
                        }
                        SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&OwnerOfResponse {
                                owner: nft_owner.to_string(),
                            })
                            .unwrap(),
                        ))
                    }
                    msgs::position_token::QueryMsg::TokenInfo { token_id } => {
                        if token_id != "1".to_string() {
                            return token_not_found_error(token_id);
                        }
                        SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&TokenInfoResponse {
                                owner: nft_owner.to_string(),
                                token_uri: None,
                            })
                            .unwrap(),
                        ))
                    }
                    msgs::position_token::QueryMsg::PositionInfo { token_id } => {
                        if token_id != "1".to_string() {
                            return token_not_found_error(token_id);
                        }
                        SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&PositionInfoResponse {
                                liquidity: Uint128::new(1000),
                            })
                            .unwrap(),
                        ))
                    }
                    _ => SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest {
                        kind: "unsupported".into(),
                    }),
                }
            }
            _ => SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest {
                kind: "unsupported".into(),
            }),
        });
        let deps = deps.as_mut();
        STATE
            .save(
                deps.storage,
                &State {
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                    router_contract: "router_contract".to_string(),
                    relayer_contract: Addr::unchecked("relayer_contract"),
                    escrow_code_id: 1,
                    lp_code_id: 2,
                    position_token_code_id: 3,
                    is_native: true,
                },
            )
            .unwrap();
        ADMIN
            .save(
                deps.storage,
                &EuclidAdmin::default(Addr::unchecked("admin")),
            )
            .unwrap();
        POOL_KEY_TO_VLP
            .save(
                deps.storage,
                pool_key.to_map_key(),
                &"vlp_address".to_string(),
            )
            .unwrap();
        VLP_TO_POSITION_TOKEN
            .save(
                deps.storage,
                "vlp_address".to_string(),
                &position_token_address,
            )
            .unwrap();
    }

    fn _initialize_state(deps: &mut DepsMut) {
        STATE
            .save(
                deps.storage,
                &State {
                    chain_uid: ChainUid::create("1".to_string()).unwrap(),
                    router_contract: "router_contract".to_string(),
                    relayer_contract: Addr::unchecked("relayer_contract"),
                    escrow_code_id: 1,
                    lp_code_id: 2,
                    position_token_code_id: 3,
                    is_native: true,
                },
            )
            .unwrap();
        ADMIN
            .save(
                deps.storage,
                &EuclidAdmin::default(Addr::unchecked("admin")),
            )
            .unwrap();
    }

    fn init(
        deps: &mut cosmwasm_std::OwnedDeps<
            cosmwasm_std::MemoryStorage,
            cosmwasm_std::testing::MockApi,
            MockQuerier,
        >,
    ) -> Response {
        let msg = InstantiateMsg {
            router_contract: "router".to_string(),
            relayer_contract: Addr::unchecked("relayer_contract"),
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            escrow_code_id: 1,
            lp_code_id: 2,
            position_token_code_id: 3,
            is_native: true,
            rate_limit_fee_recipient: Addr::unchecked("rate_limit_fee_recipient"),
            rate_limit_fee_denom: "rate_limit_fee_denom".to_string(),
            rate_limit_free_limit: Uint128::from(10u128),
        };
        let owner = deps.api.addr_make("owner");
        let info = message_info(&owner, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
    }

    // ── existing test ─────────────────────────────────────────────────────────

    #[test]
    fn test_init() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);
        // No messages are expected because the factory is not used in the tests.
        assert_eq!(0, res.messages.len());
        let owner = deps.api.addr_make("owner");
        let expected_state = State {
            router_contract: "router".to_string(),
            relayer_contract: Addr::unchecked("relayer_contract"),
            escrow_code_id: 1,
            chain_uid: ChainUid::create("1".to_string()).unwrap(),
            lp_code_id: 2,
            position_token_code_id: 3,
            is_native: true,
        };
        let state = STATE.load(&deps.storage).unwrap();
        let admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(state, expected_state);
        assert_eq!(admin, EuclidAdmin::default(owner));
    }

    // ── table-driven authorization tests ──────────────────────────────────────

    /// Identifies which address plays each role in a test case.
    /// `Alice` = position metadata owner; `Bob` = a different address.
    #[derive(Clone, Copy)]
    enum Who {
        Alice,
        Bob,
    }

    impl Who {
        fn to_address(&self, api: &cosmwasm_std::testing::MockApi) -> Addr {
            match self {
                Who::Alice => api.addr_make("alice"),
                Who::Bob => api.addr_make("bob"),
            }
        }
    }

    #[rstest]
    #[case::caller_is_owner(
        mock_pool_key(),
        Who::Alice,
        Who::Alice,
        Uint128::one(),
        Uint128::one(),
        None
    )]
    #[case::caller_does_not_own_nft(
        mock_pool_key(),
        Who::Alice,
        Who::Bob,
        Uint128::one(),
        Uint128::one(),
        Some(ContractError::Unauthorized {})
    )]
    #[case::pool_key_does_not_exist(
        mock_wrong_pool_key(),
        Who::Alice,
        Who::Bob,
        Uint128::one(),
        Uint128::one(),
        Some(ContractError::PoolDoesNotExist {})
    )]
    #[case::liquidity_delta_is_zero(
        mock_pool_key(),
        Who::Alice,
        Who::Alice,
        Uint128::one(),
        Uint128::zero(),
        Some(ContractError::ZeroAssetAmount {})
    )]
    #[case::position_id_does_not_exist(
        mock_pool_key(),
        Who::Alice,
        Who::Alice,
        Uint128::zero(), // Wrong position id
        Uint128::one(),
        Some(ContractError::NotFound { msg: "token 0 not found".to_string() })
    )]
    #[case::overflow_error(
        mock_pool_key(),
        Who::Alice,
        Who::Alice,
        Uint128::one(),
        Uint128::new(1001), // Overflow error
        Some(ContractError::InsufficientFunds {})
    )]
    fn test_remove_concentrated_liquidity_authorization(
        #[case] pool_key: PoolKey,
        #[case] nft_owner: Who,
        #[case] caller: Who,
        #[case] position_id: Uint128,
        #[case] liquidity_delta: Uint128,
        #[case] expected_error: Option<ContractError>,
    ) {
        let setup_pool_key = mock_pool_key();
        let chain_uid = ChainUid::create("1".to_string()).unwrap();

        let mut deps = mock_dependencies();

        let nft_owner = nft_owner.to_address(&deps.api);
        setup_concentrated_state(&mut deps, nft_owner, &setup_pool_key);
        let caller = caller.to_address(&deps.api);
        let info = message_info(&caller, &[]);
        let sender = CrossChainUser::new(chain_uid.clone(), caller.to_string());
        let recipient = CrossChainUser::new(chain_uid.clone(), caller.to_string());

        let result = remove_concentrated_liquidity_request(
            &mut deps.as_mut(),
            info,
            mock_env(),
            sender,
            pool_key.clone(),
            position_id,
            liquidity_delta,
            recipient,
            CrossChainConfig::default(),
        );

        match expected_error {
            Some(expected_error) => {
                let err = result.unwrap_err();
                assert!(
                    err.to_string().contains(&expected_error.to_string()),
                    "expected error {:?} but got {:?}",
                    expected_error,
                    err
                );
            }
            None => {
                assert!(result.is_ok());
            }
        }
    }

    #[rstest]
    #[case::caller_is_owner(mock_pool_key(), Who::Alice, Who::Alice, Uint128::one(), None)]
    #[case::caller_does_not_own_nft(
        mock_pool_key(),
        Who::Alice,
        Who::Bob,
        Uint128::one(),
        Some(ContractError::Unauthorized {})
    )]
    #[case::pool_key_does_not_exist(
        mock_wrong_pool_key(),
        Who::Alice,
        Who::Bob,
        Uint128::one(),
        Some(ContractError::PoolDoesNotExist {})
    )]
    #[case::position_id_does_not_exist(
        mock_pool_key(),
        Who::Alice,
        Who::Alice,
        Uint128::zero(), // Wrong position id
        Some(ContractError::NotFound { msg: "token 0 not found".to_string() })
    )]
    fn test_collect_concentrated_fees_authorization(
        #[case] pool_key: PoolKey,
        #[case] nft_owner: Who,
        #[case] caller: Who,
        #[case] position_id: Uint128,
        #[case] expected_error: Option<ContractError>,
    ) {
        let setup_pool_key = mock_pool_key();
        let chain_uid = ChainUid::create("1".to_string()).unwrap();

        let mut deps = mock_dependencies();

        let nft_owner = nft_owner.to_address(&deps.api);
        setup_concentrated_state(&mut deps, nft_owner, &setup_pool_key);

        let caller = caller.to_address(&deps.api);
        let info = message_info(&caller, &[]);
        let recipient = CrossChainUser::new(chain_uid.clone(), caller.to_string());

        let result = collect_concentrated_fees_request(
            &mut deps.as_mut(),
            info,
            mock_env(),
            pool_key.clone(),
            position_id,
            recipient,
            CrossChainConfig::default(),
        );
        match expected_error {
            Some(expected_error) => {
                let err = result.unwrap_err();
                assert!(
                    err.to_string().contains(&expected_error.to_string()),
                    "expected error {:?} but got {:?}",
                    expected_error,
                    err
                );
            }
            None => {
                assert!(result.is_ok());
            }
        }
    }
}
