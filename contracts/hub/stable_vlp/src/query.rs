use cosmwasm_std::{to_json_binary, Binary, Deps, Env, Uint256};
use euclid::chain::ChainUid;
use euclid::error::ContractError;
use euclid::msgs::vlp::stable::msg::{
    AllStablePoolsResponse, FeeResponse, GetStateResponse, StablePoolInfo, StablePoolResponse,
    TotalFeesPerDenomResponse, TotalFeesResponse, DEFAULT_AMP_FACTOR,
};
use euclid::swap::NextSwapVlp;
use euclid::token::Token;
use euclid_pool::common::calculate_amount_from_shares;
use euclid_pool::stable::simulate_swap;

use crate::state::{ADMIN, AMP_FACTOR, BALANCES, CHAIN_LP_TOKENS, STATE};
use euclid::msgs::vlp::base::{
    GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, State, VlpSimulateSwapMsg,
};
// Function to simulate swap in a query
pub fn query_simulate_swap(
    deps: Deps,
    asset_in: Token,
    amount_in: Uint256,
    next_swaps: Vec<NextSwapVlp>,
    euclid_fee_override: Option<u64>,
) -> Result<Binary, ContractError> {
    let swap_response = simulate_swap(
        deps,
        &STATE,
        &BALANCES,
        asset_in,
        amount_in,
        AMP_FACTOR.load(deps.storage).unwrap_or(DEFAULT_AMP_FACTOR),
        // Sender-aware override (resolved by the Router) so the simulated Euclid
        // fee matches execution; `None` keeps the pool's configured Euclid fee.
        euclid_fee_override,
    )?;
    let response = match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            let next_swap_response: GetSwapQueryResponse = deps.querier.query_wasm_smart(
                next_swap.vlp_address.clone(),
                &euclid::msgs::vlp::stable::msg::QueryMsg::SimulateSwap(VlpSimulateSwapMsg {
                    asset: swap_response.asset_out,
                    asset_amount: swap_response.amount_out,
                    swaps: forward_swaps.to_vec(),
                    // Forward the override so it applies on every simulated hop.
                    euclid_fee_override,
                }),
            )?;
            Ok(to_json_binary(&next_swap_response)?)
        }
        None => Ok(to_json_binary(&swap_response)?),
    };
    response
}

// Function to query the total liquidity
pub fn query_liquidity(deps: Deps, _env: Env) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    let pair = state.pair.clone();
    Ok(to_json_binary(&GetLiquidityQueryResponse {
        pair,
        token_1_reserve: BALANCES
            .may_load(deps.storage, state.pair.token_1)?
            .unwrap_or_default(),
        token_2_reserve: BALANCES
            .may_load(deps.storage, state.pair.token_2)?
            .unwrap_or_default(),
        total_lp_tokens: state.total_lp_tokens,
    })?)
}

// Function to query fee of the contract
pub fn query_fee(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&FeeResponse { fee: state.fee })?)
}

// Function to query total fees collected of the contract
pub fn query_total_fees_collected(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&TotalFeesResponse {
        total_fees: state.total_fees_collected,
    })?)
}

pub fn query_total_fees_per_denom(deps: Deps, denom: String) -> Result<Binary, ContractError> {
    let total_fees_collected = STATE.load(deps.storage)?.total_fees_collected;

    let lp_fees = total_fees_collected.lp_fees.get_fee(denom.as_str());
    let euclid_fees = total_fees_collected.euclid_fees.get_fee(denom.as_str());

    Ok(to_json_binary(&TotalFeesPerDenomResponse {
        lp_fees,
        euclid_fees,
    })?)
}

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetStateResponse {
        pair: state.pair,
        router: state.router,
        virtual_balance_contract: state.virtual_balance_contract,
        fee: state.fee,
        total_fees_collected: state.total_fees_collected,
        last_updated: state.last_updated,
        total_lp_tokens: state.total_lp_tokens,
        pool_config: PoolConfig::Stable {
            amp_factor: AMP_FACTOR.may_load(deps.storage)?,
        },
    })?)
}

pub fn query_admin(deps: Deps) -> Result<Binary, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    Ok(to_json_binary(&admin)?)
}

// Function to query a Euclid Pool Information for this pair
pub fn query_pool(deps: Deps, chain_uid: ChainUid) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;

    let chain_lp_tokens = CHAIN_LP_TOKENS.load(deps.storage, chain_uid)?;

    let reserve_1 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;

    let reserve_2 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;

    let pool = get_pool(&state, chain_lp_tokens, reserve_1, reserve_2)?;

    Ok(to_json_binary(&pool)?)
}
// Function to query all Euclid Pool Information
pub fn query_all_pools(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;

    let reserve_1 = BALANCES.load(deps.storage, state.pair.token_1.clone())?;

    let reserve_2 = BALANCES.load(deps.storage, state.pair.token_2.clone())?;

    let pools: Result<_, ContractError> = CHAIN_LP_TOKENS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (chain_uid, chain_lp_tokens) = item?;
            let pool = get_pool(&state, chain_lp_tokens, reserve_1, reserve_2)?;

            Ok::<StablePoolInfo, ContractError>(StablePoolInfo { chain_uid, pool })
        })
        .collect();

    Ok(to_json_binary(&AllStablePoolsResponse { pools: pools? })?)
}

fn get_pool(
    state: &State,
    chain_lp_tokens: Uint256,
    reserve_1: Uint256,
    reserve_2: Uint256,
) -> Result<StablePoolResponse, ContractError> {
    Ok(StablePoolResponse {
        reserve_1: calculate_amount_from_shares(reserve_1, chain_lp_tokens, state.total_lp_tokens)
            .unwrap_or(Uint256::zero()),
        reserve_2: calculate_amount_from_shares(reserve_2, chain_lp_tokens, state.total_lp_tokens)
            .unwrap_or(Uint256::zero()),
        lp_shares: chain_lp_tokens,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        contract::{execute, query},
        query::query_simulate_swap,
        state::{ADMIN, AMP_FACTOR, BALANCES, STATE},
        testing::helpers::{
            chain1, chain2, default_fee, default_pair, init, register_pool, seed_liquidity,
            sender_on_chain1, token1, token2,
        },
    };
    use cosmwasm_std::{
        from_json,
        testing::{message_info, mock_dependencies, mock_env},
        Addr, Uint256, Uint64,
    };
    use euclid::{
        admin::EuclidAdmin,
        cross_chain_user::CrossChainUser,
        error::ContractError,
        fee::{DenomFees, Fee, TotalFees},
        msgs::vlp::{
            base::{GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, State},
            stable::msg::{
                AllStablePoolsResponse, ExecuteMsg, FeeResponse, GetStateResponse, InstantiateMsg,
                QueryMsg, StablePoolResponse, TotalFeesPerDenomResponse, TotalFeesResponse,
                DEFAULT_AMP_FACTOR,
            },
        },
        token::Pair,
    };
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Query: State
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_state_returns_correct_data() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::State {}).unwrap();
        let state: GetStateResponse = from_json(res).unwrap();

        let router = deps.api.addr_make("router");
        assert_eq!(state.pair, default_pair());
        assert_eq!(state.router, router);
        assert_eq!(state.fee, default_fee());
        assert_eq!(state.total_lp_tokens, Uint256::zero());
        assert!(matches!(state.pool_config, PoolConfig::Stable { .. }));

        if let PoolConfig::Stable { amp_factor } = state.pool_config {
            assert_eq!(amp_factor, Some(Uint64::from(1000u64)));
        }
    }

    // -----------------------------------------------------------------------
    // Query: Admin
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_admin_returns_correct_admin() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAdmin {}).unwrap();
        let admin: EuclidAdmin = from_json(res).unwrap();

        let expected_admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        assert_eq!(admin, expected_admin);
    }

    // -----------------------------------------------------------------------
    // Query: Fee
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_fee_returns_current_fee() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Fee {}).unwrap();
        let fee_resp: FeeResponse = from_json(res).unwrap();
        assert_eq!(fee_resp.fee, default_fee());
    }

    #[test]
    fn test_query_fee_reflects_update() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let admin = deps.api.addr_make("admin");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::UpdateFee {
            lp_fee_bps: Some(42),
            euclid_fee_bps: Some(7),
            recipient: None,
        };
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        let res = query(deps.as_ref(), env, QueryMsg::Fee {}).unwrap();
        let fee_resp: FeeResponse = from_json(res).unwrap();
        assert_eq!(fee_resp.fee.lp_fee_bps, 42);
        assert_eq!(fee_resp.fee.euclid_fee_bps, 7);
    }

    // -----------------------------------------------------------------------
    // Query: TotalFeesCollected / TotalFeesPerDenom
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_total_fees_collected_initially_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::TotalFeesCollected {}).unwrap();
        let fees: TotalFeesResponse = from_json(res).unwrap();
        assert!(fees.total_fees.lp_fees.totals.is_empty());
        assert!(fees.total_fees.euclid_fees.totals.is_empty());
    }

    #[test]
    fn test_query_total_fees_per_denom_returns_zero_for_unknown_denom() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::TotalFeesPerDenom {
                denom: "token1".to_string(),
            },
        )
        .unwrap();
        let resp: TotalFeesPerDenomResponse = from_json(res).unwrap();
        assert_eq!(resp.lp_fees, Uint256::zero());
        assert_eq!(resp.euclid_fees, Uint256::zero());
    }

    #[test]
    fn test_query_total_fees_per_denom_reflects_swap_fees() {
        let mut deps = mock_dependencies();
        let router = deps.api.addr_make("router");
        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        let msg = InstantiateMsg {
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            pair: default_pair(),
            fee: Fee::new(30, 0, CrossChainUser::new(chain1(), "addr".to_string())),
            execute: None,
            admin,
            amp_factor: Some(Uint64::from(1000u64)),
        };
        let info = message_info(&router, &[]);
        crate::contract::instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        seed_liquidity(&mut deps, 10_000_000_000);

        let info = message_info(&router, &[]);
        let swap_msg = ExecuteMsg::Swap(euclid::msgs::vlp::base::VlpSwapMsg {
            sender: sender_on_chain1(),
            tx_id: "swap-fee-tx".to_string(),
            asset_in: token1(),
            amount_in: Uint256::from(10_000u128),
            min_token_out: Uint256::from(1u128),
            next_swaps: vec![],
            test_fail: None,
            euclid_fee_override: None,
        });
        execute(deps.as_mut(), mock_env(), info, swap_msg).unwrap();

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::TotalFeesPerDenom {
                denom: "token1".to_string(),
            },
        )
        .unwrap();
        let resp: TotalFeesPerDenomResponse = from_json(res).unwrap();
        assert_eq!(resp.lp_fees, Uint256::from(30u128));
        assert_eq!(resp.euclid_fees, Uint256::zero());
    }

    // -----------------------------------------------------------------------
    // Query: Liquidity
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_liquidity_initially_zero() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Liquidity {}).unwrap();
        let liq: GetLiquidityQueryResponse = from_json(res).unwrap();
        assert_eq!(liq.token_1_reserve, Uint256::zero());
        assert_eq!(liq.token_2_reserve, Uint256::zero());
        assert_eq!(liq.total_lp_tokens, Uint256::zero());
    }

    #[test]
    fn test_query_liquidity_after_add() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let reserve = 10_000_000_000u128;
        seed_liquidity(&mut deps, reserve);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Liquidity {}).unwrap();
        let liq: GetLiquidityQueryResponse = from_json(res).unwrap();
        assert_eq!(liq.token_1_reserve, Uint256::from(reserve));
        assert_eq!(liq.token_2_reserve, Uint256::from(reserve));
        assert!(liq.total_lp_tokens > Uint256::zero());
        assert_eq!(liq.pair, default_pair());
    }

    // -----------------------------------------------------------------------
    // Query: Pool / GetAllPools
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_pool_for_registered_chain() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::Pool {
                chain_uid: chain1(),
            },
        )
        .unwrap();
        let pool: StablePoolResponse = from_json(res).unwrap();
        assert!(pool.lp_shares > Uint256::zero());
    }

    #[test]
    fn test_query_pool_for_unregistered_chain_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let err = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::Pool {
                chain_uid: chain2(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn test_query_all_pools_empty_when_no_pools() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllPools {}).unwrap();
        let all: AllStablePoolsResponse = from_json(res).unwrap();
        assert!(all.pools.is_empty());
    }

    #[test]
    fn test_query_all_pools_returns_registered_pools() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllPools {}).unwrap();
        let all: AllStablePoolsResponse = from_json(res).unwrap();
        assert_eq!(all.pools.len(), 1);
        assert_eq!(all.pools[0].chain_uid, chain1());
    }

    #[test]
    fn test_query_all_pools_multiple_chains() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        register_pool(&mut deps, chain2(), "reg-tx-2");

        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetAllPools {}).unwrap();
        let all: AllStablePoolsResponse = from_json(res).unwrap();
        assert_eq!(all.pools.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Query: SimulateSwap
    // -----------------------------------------------------------------------

    #[test]
    fn test_simulate_swap_balanced_pool_minimal_spread() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let res: GetSwapQueryResponse = from_json(
            query_simulate_swap(
                deps.as_ref(),
                token1(),
                Uint256::from(1_000u128),
                vec![],
                None,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res.asset_out, token2());
        assert!(res.amount_out > Uint256::zero());
        assert!(res.amount_out <= Uint256::from(1000u128));
    }

    #[test]
    fn test_simulate_swap_zero_amount_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let err = query_simulate_swap(deps.as_ref(), token1(), Uint256::zero(), vec![], None)
            .unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_simulate_swap_unknown_asset_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        seed_liquidity(&mut deps, 10_000_000_000);

        let err = query_simulate_swap(
            deps.as_ref(),
            euclid::token::Token::create("xtoken".to_string()).unwrap(),
            Uint256::from(1_000u128),
            vec![],
            None,
        )
        .unwrap_err();
        assert_eq!(err, ContractError::AssetDoesNotExist {});
    }

    #[test]
    fn test_simulate_swap_imbalanced_pool_spread() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let pair = Pair {
            token_1: euclid::token::Token::create("uatom".to_string()).unwrap(),
            token_2: euclid::token::Token::create("uosmo".to_string()).unwrap(),
        };

        let state = State {
            pair: pair.clone(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            fee: Fee::new(30, 0, CrossChainUser::new(chain1(), "addr".to_string())),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: env.block.time.seconds(),
            total_lp_tokens: Uint256::from(1000u128),
        };

        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN.save(deps.as_mut().storage, &admin).unwrap();
        AMP_FACTOR
            .save(deps.as_mut().storage, &DEFAULT_AMP_FACTOR)
            .unwrap();

        BALANCES
            .save(
                deps.as_mut().storage,
                pair.token_1.clone(),
                &Uint256::from(1000u128),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                pair.token_2.clone(),
                &Uint256::from(500u128),
            )
            .unwrap();

        let response: GetSwapQueryResponse = from_json(
            query_simulate_swap(
                deps.as_ref(),
                pair.token_1,
                Uint256::from(100u128),
                vec![],
                None,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(response.asset_out, pair.token_2);
        assert_eq!(response.amount_out, Uint256::from(90u128));
        assert_eq!(response.spread_amount, Uint256::from(10u128));
    }

    #[test]
    fn test_simulate_swap_large_reserve_ratio() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let pair = Pair {
            token_1: euclid::token::Token::create("uatom".to_string()).unwrap(),
            token_2: euclid::token::Token::create("uosmo".to_string()).unwrap(),
        };

        let state = State {
            pair: pair.clone(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked("virtual_balance_contract"),
            fee: Fee::new(30, 0, CrossChainUser::new(chain1(), "addr".to_string())),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: env.block.time.seconds(),
            total_lp_tokens: Uint256::from(1000u128),
        };

        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN.save(deps.as_mut().storage, &admin).unwrap();
        AMP_FACTOR
            .save(deps.as_mut().storage, &DEFAULT_AMP_FACTOR)
            .unwrap();

        BALANCES
            .save(
                deps.as_mut().storage,
                pair.token_1.clone(),
                &Uint256::from(9971294131355738400u128),
            )
            .unwrap();
        BALANCES
            .save(
                deps.as_mut().storage,
                pair.token_2.clone(),
                &Uint256::from(64769345018139098454u128),
            )
            .unwrap();

        let response: GetSwapQueryResponse = from_json(
            query_simulate_swap(
                deps.as_ref(),
                pair.token_1,
                Uint256::from(10000000000000000u128),
                vec![],
                None,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(response.asset_out, pair.token_2);
        assert_eq!(response.amount_out, Uint256::from(15317895796684530u128));
        assert!(response.spread_amount > Uint256::zero());
    }
}
