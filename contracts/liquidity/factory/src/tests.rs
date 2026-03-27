#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::execute::pool::{
        collect_concentrated_fees_request, remove_concentrated_liquidity_request,
    };
    use crate::state::{
        pool_key_to_map_key, ConcentratedPositionMetadata, State, ADMIN, POOL_KEY_TO_VLP,
        POSITION_ID_TO_METADATA, POSITION_TOKEN_CONTRACT, STATE,
    };

    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockQuerier};
    use cosmwasm_std::{
        to_json_binary, Addr, ContractResult, DepsMut, Response, SystemResult, Uint128,
    };
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::error::ContractError;
    use euclid::msgs::cross_chain_config::CrossChainConfig;
    use euclid::msgs::factory::InstantiateMsg;
    use euclid::msgs::position_token::OwnerOfResponse;
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

    /// Saves STATE, ADMIN, POSITION_TOKEN_CONTRACT, and one position metadata entry.
    fn setup_concentrated_state(deps: &mut DepsMut, owner: &Addr, pool_key: &PoolKey) {
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
        POSITION_TOKEN_CONTRACT
            .save(deps.storage, &Addr::unchecked("position_nft"))
            .unwrap();
        POSITION_ID_TO_METADATA
            .save(
                deps.storage,
                1u128,
                &ConcentratedPositionMetadata {
                    owner: owner.clone(),
                    pool_key: pool_key.clone(),
                    liquidity: Uint128::from(100u128),
                    vlp_address: "vlp".to_string(),
                },
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
        assert_eq!(1, res.messages.len());
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

    struct AuthCase {
        name: &'static str,
        /// When false the position metadata is not saved, exercising the
        /// "position not found" path before the ownership check is reached.
        setup_position: bool,
        /// Who the NFT contract reports as current owner (ignored when
        /// `setup_position` is false because the querier is never called).
        nft_owner: Who,
        /// Who sends the transaction.
        caller: Who,
        check_err: fn(&ContractError) -> bool,
    }

    /// Shared cases for both `remove_concentrated_liquidity_request` and
    /// `collect_concentrated_fees_request`.
    fn auth_cases() -> Vec<AuthCase> {
        vec![
            AuthCase {
                name: "caller does not own NFT",
                setup_position: true,
                nft_owner: Who::Alice,
                caller: Who::Bob,
                check_err: |e| matches!(e, ContractError::Unauthorized {}),
            },
            // Critical regression guard: position_meta.owner (alice) differs from
            // owner_resp.owner (bob) because alice transferred the NFT to bob.
            // Alice must be rejected even though factory state still names her.
            // This case fails if the auth check is changed to use position_meta.owner.
            AuthCase {
                name: "nft transferred — original metadata owner rejected",
                setup_position: true,
                nft_owner: Who::Bob,
                caller: Who::Alice,
                check_err: |e| matches!(e, ContractError::Unauthorized {}),
            },
            AuthCase {
                name: "position metadata not found",
                setup_position: false,
                nft_owner: Who::Alice, // irrelevant — querier is never reached
                caller: Who::Alice,
                check_err: |e| e.to_string().contains("Position not found"),
            },
        ]
    }

    #[test]
    fn test_remove_concentrated_liquidity_authorization() {
        let pool_key = mock_pool_key();
        let chain_uid = ChainUid::create("1".to_string()).unwrap();

        for case in auth_cases() {
            let mut deps = mock_dependencies();
            let alice = Who::Alice.to_address(&deps.api);

            if case.setup_position {
                setup_concentrated_state(&mut deps.as_mut(), &alice, &pool_key);
                let nft_owner_str = case.nft_owner.to_address(&deps.api).to_string();
                deps.querier.update_wasm(move |_| {
                    SystemResult::Ok(ContractResult::Ok(
                        to_json_binary(&OwnerOfResponse {
                            owner: nft_owner_str.clone(),
                        })
                        .unwrap(),
                    ))
                });
            } else {
                STATE
                    .save(
                        deps.as_mut().storage,
                        &State {
                            chain_uid: chain_uid.clone(),
                            router_contract: "router_contract".to_string(),
                            relayer_contract: Addr::unchecked("relayer_contract"),
                            escrow_code_id: 1,
                            lp_code_id: 2,
                            position_token_code_id: 3,
                            is_native: true,
                        },
                    )
                    .unwrap();
                POSITION_TOKEN_CONTRACT
                    .save(deps.as_mut().storage, &Addr::unchecked("position_nft"))
                    .unwrap();
            }

            let caller = case.caller.to_address(&deps.api);
            let info = message_info(&caller, &[]);
            let sender = CrossChainUser::new(chain_uid.clone(), caller.to_string());
            let recipient = CrossChainUser::new(chain_uid.clone(), caller.to_string());

            let err = remove_concentrated_liquidity_request(
                &mut deps.as_mut(),
                info,
                mock_env(),
                sender,
                pool_key.clone(),
                Uint128::one(),
                Uint128::one(),
                recipient,
                CrossChainConfig::default(),
            )
            .unwrap_err();

            assert!(
                (case.check_err)(&err),
                "case '{}' failed: unexpected error {:?}",
                case.name,
                err
            );
        }
    }

    #[test]
    fn test_collect_concentrated_fees_authorization() {
        let pool_key = mock_pool_key();
        let chain_uid = ChainUid::create("1".to_string()).unwrap();

        for case in auth_cases() {
            let mut deps = mock_dependencies();
            let alice = Who::Alice.to_address(&deps.api);

            if case.setup_position {
                setup_concentrated_state(&mut deps.as_mut(), &alice, &pool_key);
            } else {
                STATE
                    .save(
                        deps.as_mut().storage,
                        &State {
                            chain_uid: chain_uid.clone(),
                            router_contract: "router_contract".to_string(),
                            relayer_contract: Addr::unchecked("relayer_contract"),
                            escrow_code_id: 1,
                            lp_code_id: 2,
                            position_token_code_id: 3,
                            is_native: true,
                        },
                    )
                    .unwrap();
            }

            // collect_fees always checks POOL_KEY_TO_VLP before the ownership check.
            POOL_KEY_TO_VLP
                .save(
                    deps.as_mut().storage,
                    pool_key_to_map_key(&pool_key),
                    &"vlp_address".to_string(),
                )
                .unwrap();

            if case.setup_position {
                let nft_owner_str = case.nft_owner.to_address(&deps.api).to_string();
                deps.querier.update_wasm(move |_| {
                    SystemResult::Ok(ContractResult::Ok(
                        to_json_binary(&OwnerOfResponse {
                            owner: nft_owner_str.clone(),
                        })
                        .unwrap(),
                    ))
                });
            }

            let caller = case.caller.to_address(&deps.api);
            let info = message_info(&caller, &[]);
            let recipient = CrossChainUser::new(chain_uid.clone(), caller.to_string());

            let err = collect_concentrated_fees_request(
                &mut deps.as_mut(),
                info,
                mock_env(),
                pool_key.clone(),
                Uint128::one(),
                recipient,
                CrossChainConfig::default(),
            )
            .unwrap_err();

            assert!(
                (case.check_err)(&err),
                "case '{}' failed: unexpected error {:?}",
                case.name,
                err
            );
        }
    }
}
