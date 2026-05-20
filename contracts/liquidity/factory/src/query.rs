use cosmwasm_std::{to_json_binary, Addr, Binary, Deps, Env, Order, Uint256};
use cw_storage_plus::Bound;
use euclid::{
    chain::{ChainType, CosmosChain},
    error::ContractError,
    msgs::factory::{
        AllPoolsResponse, AllTokensResponse, FeeBracket, GetEscrowResponse, GetLPTokenResponse,
        GetPendingLiquidityResponse, GetPendingRemoveLiquidityResponse,
        GetPendingSingleSidedLiquidityResponse, GetPendingSwapsResponse, GetRateLimitStateResponse,
        GetUserRateLimitResponse, GetVlpResponse, PartnerFeesCollectedPerDenomResponse,
        PartnerFeesCollectedResponse, PoolVlpResponse, StateResponse,
    },
    token::{Pair, Token},
    utils::pagination::Pagination,
};

use crate::{
    rate_limit::{RATE_LIMIT_STATE, USER_FREE_LIMIT, USER_PENDING_PACKETS_COUNT},
    state::{
        ADMIN, FEE_STATE, PAIR_TO_VLP, PENDING_ADD_LIQUIDITY, PENDING_REMOVE_LIQUIDITY,
        PENDING_SINGLE_SIDED_LIQUIDITY, PENDING_SWAPS, STATE, TOKEN_TO_ESCROW, VLP_TO_LP_TOKEN,
    },
};

// Returns the VLP address
pub fn get_vlp(deps: Deps, pair: Pair) -> Result<Binary, ContractError> {
    let vlp_address = PAIR_TO_VLP.load(deps.storage, pair.get_tupple())?;
    Ok(to_json_binary(&GetVlpResponse { vlp_address })?)
}

// Returns the total partner fees collected
pub fn get_partner_fees_collected(deps: Deps) -> Result<Binary, ContractError> {
    let fee_state = FEE_STATE.load(deps.storage)?;
    Ok(to_json_binary(&PartnerFeesCollectedResponse {
        total: fee_state.partner_fees_collected,
    })?)
}

pub fn get_partner_fees_collected_per_denom(
    deps: Deps,
    denom: String,
) -> Result<Binary, ContractError> {
    let partner_fees_collected = FEE_STATE.load(deps.storage)?.partner_fees_collected;

    Ok(to_json_binary(&PartnerFeesCollectedPerDenomResponse {
        total: partner_fees_collected.get_fee(denom.as_str()),
    })?)
}

// Returns the LP token address
pub fn get_lp_token_address(deps: Deps, vlp: String) -> Result<Binary, ContractError> {
    let token_address = VLP_TO_LP_TOKEN.load(deps.storage, vlp)?;
    Ok(to_json_binary(&GetLPTokenResponse { token_address })?)
}

// Returns the Escrow address alongside allowed denoms if available
pub fn get_escrow(deps: Deps, token_id: String) -> Result<Binary, ContractError> {
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, Token::create(token_id)?)?;
    let mut response = GetEscrowResponse {
        escrow_address: escrow_address.clone(),
        denoms: vec![],
    };
    if let Some(escrow_address) = escrow_address {
        let denoms: euclid::msgs::escrow::AllowedDenomsResponse = deps.querier.query_wasm_smart(
            escrow_address,
            &euclid::msgs::escrow::QueryMsg::AllowedDenoms {},
        )?;
        response.denoms = denoms.denoms;
    }
    Ok(to_json_binary(&response)?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let admin = ADMIN.load(deps.storage)?;
    Ok(to_json_binary(&StateResponse {
        chain_uid: state.chain_uid,
        router_contract: state.router_contract,
        relayer_contract: state.relayer_contract,
        admin,
        escrow_code_id: state.escrow_code_id,
        lp_code_id: state.lp_code_id,
        is_native: state.is_native,
    })?)
}
pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let pools: Vec<PoolVlpResponse> = PAIR_TO_VLP
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (pair_tokens, vlp) = item?;
            Ok(PoolVlpResponse {
                pair: Pair::new(Token::create(pair_tokens.0)?, Token::create(pair_tokens.1)?)?,
                vlp,
            })
        })
        .collect::<Result<_, ContractError>>()?;

    to_json_binary(&AllPoolsResponse { pools }).map_err(Into::into)
}

pub fn query_all_tokens(deps: Deps) -> Result<Binary, ContractError> {
    let tokens = TOKEN_TO_ESCROW
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .flatten()
        .collect();

    to_json_binary(&AllTokensResponse { tokens }).map_err(Into::into)
}

// Returns the pending swaps for this pair with pagination
pub fn pending_swaps(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint256>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.map(Bound::inclusive);
    let max = pagination.max.map(Bound::inclusive);

    // Fetch pending swaps for user
    let pending_swaps = PENDING_SWAPS
        .prefix(user)
        .range(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .map(|k| k.unwrap().1)
        .collect();

    Ok(to_json_binary(&GetPendingSwapsResponse { pending_swaps })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_liquidity(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint256>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.map(Bound::inclusive);
    let max = pagination.max.map(Bound::inclusive);

    let pending_add_liquidity = PENDING_ADD_LIQUIDITY
        .prefix(user)
        .range(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .flat_map(|k| -> Result<_, ContractError> { Ok(k?.1) })
        .collect();

    Ok(to_json_binary(&GetPendingLiquidityResponse {
        pending_add_liquidity,
    })?)
}

// Returns the pending liquidity transactions for a user with pagination
pub fn pending_remove_liquidity(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint256>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.map(Bound::inclusive);
    let max = pagination.max.map(Bound::inclusive);

    let pending_remove_liquidity = PENDING_REMOVE_LIQUIDITY
        .prefix(user)
        .range(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .flat_map(|k| -> Result<_, ContractError> { Ok(k?.1) })
        .collect();

    Ok(to_json_binary(&GetPendingRemoveLiquidityResponse {
        pending_remove_liquidity,
    })?)
}

// Returns pending single-sided add-liquidity requests for a user.
pub fn pending_single_sided_liquidity(
    deps: Deps,
    user: Addr,
    pagination: Pagination<Uint256>,
) -> Result<Binary, ContractError> {
    let min = pagination.min.map(Bound::inclusive);
    let max = pagination.max.map(Bound::inclusive);

    let pending_single_sided_liquidity = PENDING_SINGLE_SIDED_LIQUIDITY
        .prefix(user)
        .range(deps.storage, min, max, Order::Ascending)
        .skip(pagination.skip.unwrap_or(0) as usize)
        .take(pagination.limit.unwrap_or(10) as usize)
        .flat_map(|k| -> Result<_, ContractError> { Ok(k?.1) })
        .collect();

    Ok(to_json_binary(&GetPendingSingleSidedLiquidityResponse {
        pending_single_sided_liquidity,
    })?)
}

pub fn get_rate_limit_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = RATE_LIMIT_STATE.load(deps.storage)?;
    let fee_brackets = state
        .fee_brackets
        .into_iter()
        .map(|b| FeeBracket {
            threshold: b.threshold,
            fee: b.fee,
        })
        .collect();
    Ok(to_json_binary(&GetRateLimitStateResponse {
        free_limit: state.free_limit,
        fee_brackets,
    })?)
}

pub fn get_user_rate_limit(deps: Deps, user: Addr) -> Result<Binary, ContractError> {
    let free_limit = USER_FREE_LIMIT.may_load(deps.storage, user.clone())?;
    let pending_packets = USER_PENDING_PACKETS_COUNT
        .may_load(deps.storage, user.clone())?
        .unwrap_or(0);
    Ok(to_json_binary(&GetUserRateLimitResponse {
        user,
        free_limit,
        pending_packets,
    })?)
}

pub fn get_chain_type(deps: Deps, env: &Env) -> Result<ChainType, ContractError> {
    let state = STATE.load(deps.storage)?;
    if state.is_native {
        Ok(ChainType::Native {})
    } else {
        Ok(ChainType::Cosmos(CosmosChain {
            chain_id: env.block.chain_id.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, Addr, ContractResult, SystemResult, Uint128, Uint256, WasmQuery,
    };
    use euclid::{
        chain::ChainUid,
        msgs::{
            escrow::AllowedDenomsResponse,
            factory::{AllPoolsResponse, AllTokensResponse, QueryMsg, StateResponse},
        },
        token::{Pair, Token, TokenType},
        utils::pagination::Pagination,
    };

    use crate::{
        contract::query,
        testing::helpers::{init, seed_escrow, seed_vlp, TEST_RELAYER, TEST_ROUTER},
    };

    // -----------------------------------------------------------------------
    // Query: GetState
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_state() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetState {}).unwrap();
        let state: StateResponse = cosmwasm_std::from_json(res).unwrap();

        assert_eq!(state.router_contract, TEST_ROUTER);
        assert_eq!(state.relayer_contract, Addr::unchecked(TEST_RELAYER));
        assert_eq!(state.escrow_code_id, 10);
        assert_eq!(state.lp_code_id, 11);
        assert!(!state.is_native);
    }

    // -----------------------------------------------------------------------
    // Query: GetAllPools (empty + seeded)
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_all_pools_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllPools {}).unwrap();
        let pools: AllPoolsResponse = cosmwasm_std::from_json(res).unwrap();
        assert!(pools.pools.is_empty());
    }

    #[test]
    fn test_query_all_pools_seeded() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "aaa", "bbb", "vlp_addr");

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllPools {}).unwrap();
        let pools: AllPoolsResponse = cosmwasm_std::from_json(res).unwrap();
        assert_eq!(pools.pools.len(), 1);
        assert_eq!(pools.pools[0].vlp, "vlp_addr");
    }

    // -----------------------------------------------------------------------
    // Query: GetAllTokens (empty + seeded)
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_all_tokens_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllTokens {}).unwrap();
        let tokens: AllTokensResponse = cosmwasm_std::from_json(res).unwrap();
        assert!(tokens.tokens.is_empty());
    }

    #[test]
    fn test_query_all_tokens_seeded() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_escrow(&mut deps, "usdc", "escrow1");
        seed_escrow(&mut deps, "eth", "escrow2");

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllTokens {}).unwrap();
        let tokens: AllTokensResponse = cosmwasm_std::from_json(res).unwrap();
        assert_eq!(tokens.tokens.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Query: GetEscrow
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_get_escrow_not_found_returns_none() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetEscrow {
                token_id: "unknown".to_string(),
            },
        )
        .unwrap();
        let escrow: euclid::msgs::factory::GetEscrowResponse =
            cosmwasm_std::from_json(res).unwrap();
        assert!(escrow.escrow_address.is_none());
        assert!(escrow.denoms.is_empty());
    }

    #[test]
    fn test_query_get_escrow_found_returns_address_and_denoms() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_escrow(&mut deps, "usdc", "escrow_addr");

        deps.querier.update_wasm(|q| match q {
            WasmQuery::Smart { .. } => {
                let resp = AllowedDenomsResponse {
                    denoms: vec![TokenType::Native {
                        denom: "uusdc".to_string(),
                        decimals: None,
                    }],
                };
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
            }
            _ => panic!("unexpected query"),
        });

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetEscrow {
                token_id: "usdc".to_string(),
            },
        )
        .unwrap();
        let escrow: euclid::msgs::factory::GetEscrowResponse =
            cosmwasm_std::from_json(res).unwrap();
        assert_eq!(escrow.escrow_address, Some(Addr::unchecked("escrow_addr")));
        assert_eq!(escrow.denoms.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Query: GetVlp
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_get_vlp_not_found_errors() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        let err = query(deps.as_ref(), mock_env(), QueryMsg::GetVlp { pair }).unwrap_err();
        assert!(matches!(err, euclid::error::ContractError::Std(_)));
    }

    #[test]
    fn test_query_get_vlp_found() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_vlp(&mut deps, "aaa", "bbb", "vlp_addr");

        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetVlp { pair }).unwrap();
        let vlp: euclid::msgs::factory::GetVlpResponse = cosmwasm_std::from_json(res).unwrap();
        assert_eq!(vlp.vlp_address, "vlp_addr");
    }

    // -----------------------------------------------------------------------
    // Query: GetPartnerFeesCollected
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_partner_fees_collected_starts_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::GetPartnerFeesCollected {},
        )
        .unwrap();
        let fees: euclid::msgs::factory::PartnerFeesCollectedResponse =
            cosmwasm_std::from_json(res).unwrap();
        assert!(fees.total.totals.is_empty());
    }

    // -----------------------------------------------------------------------
    // Query: PendingSwapsUser / PendingLiquidity / PendingRemoveLiquidity (empty)
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_pending_swaps_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = deps.api.addr_make("user");

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::PendingSwapsUser {
                user: user.clone(),
                pagination: Pagination {
                    min: None,
                    max: None,
                    skip: None,
                    limit: None,
                },
            },
        )
        .unwrap();
        let pending: euclid::msgs::factory::GetPendingSwapsResponse =
            cosmwasm_std::from_json(res).unwrap();
        assert!(pending.pending_swaps.is_empty());
    }

    #[test]
    fn test_query_pending_liquidity_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = deps.api.addr_make("user");

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::PendingLiquidity {
                user: user.clone(),
                pagination: Pagination {
                    min: None,
                    max: None,
                    skip: None,
                    limit: None,
                },
            },
        )
        .unwrap();
        let pending: euclid::msgs::factory::GetPendingLiquidityResponse =
            cosmwasm_std::from_json(res).unwrap();
        assert!(pending.pending_add_liquidity.is_empty());
    }

    #[test]
    fn test_query_pending_remove_liquidity_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = deps.api.addr_make("user");

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::PendingRemoveLiquidity {
                user: user.clone(),
                pagination: Pagination {
                    min: None,
                    max: None,
                    skip: None,
                    limit: None,
                },
            },
        )
        .unwrap();
        let pending: euclid::msgs::factory::GetPendingRemoveLiquidityResponse =
            cosmwasm_std::from_json(res).unwrap();
        assert!(pending.pending_remove_liquidity.is_empty());
    }

    // -----------------------------------------------------------------------
    // State invariant: ChainType derived from is_native flag
    // -----------------------------------------------------------------------

    #[test]
    fn test_chain_type_cosmos_when_not_native() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let state = crate::state::STATE.load(&deps.storage).unwrap();
        assert!(!state.is_native);

        let chain_type = super::get_chain_type(deps.as_ref(), &mock_env()).unwrap();
        assert!(matches!(chain_type, euclid::chain::ChainType::Cosmos(_)));
    }

    #[test]
    fn test_chain_type_native_when_native() {
        use euclid::msgs::factory::InstantiateMsg;

        use crate::testing::helpers::{
            TEST_CHAIN_UID, TEST_RATE_LIMIT_FEE_RECIPIENT, TEST_RELAYER, TEST_ROUTER,
        };

        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("sender");
        let info = message_info(&sender, &[]);
        let msg = InstantiateMsg {
            router_contract: TEST_ROUTER.to_string(),
            chain_uid: ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
            escrow_code_id: 10,
            lp_code_id: 11,
            is_native: true,
            relayer_contract: Addr::unchecked(TEST_RELAYER),
            rate_limit_fee_recipient: Addr::unchecked(TEST_RATE_LIMIT_FEE_RECIPIENT),
            rate_limit_fee_denom: "uusd".to_string(),
            rate_limit_free_limit: Uint256::from(100u128),
        };
        crate::contract::instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let chain_type = super::get_chain_type(deps.as_ref(), &mock_env()).unwrap();
        assert!(matches!(chain_type, euclid::chain::ChainType::Native {}));
    }
}
