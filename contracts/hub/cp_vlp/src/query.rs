use cosmwasm_std::{ensure, to_json_binary, Binary, Deps, Env, Uint256};
use euclid::chain::ChainUid;
use euclid::error::ContractError;
use euclid::msgs::vlp::base::{
    GetLiquidityQueryResponse, GetSwapQueryResponse, PoolConfig, State, VlpSimulateSwapMsg,
};
use euclid::swap::NextSwapVlp;
use euclid::token::{Pair, PairWithAmount, Token};
use euclid_pool::common::calculate_amount_from_shares;
use euclid_pool::cp::simulate_swap;

use crate::state::{ADMIN, BALANCES, CHAIN_LP_TOKENS, STATE};
use euclid::msgs::vlp::cp::msg::{
    AllPoolsResponse, FeeResponse, GetStateResponse, PoolInfo, PoolResponse,
    TotalFeesPerDenomResponse, TotalFeesResponse,
};

// Function to simulate swap in a query
pub fn query_simulate_swap(
    deps: Deps,
    asset_in: Token,
    amount_in: Uint256,
    next_swaps: Vec<NextSwapVlp>,
) -> Result<Binary, ContractError> {
    // Verify that the asset amount is non-zero
    ensure!(!amount_in.is_zero(), ContractError::ZeroAssetAmount {});

    let state = STATE.load(deps.storage)?;

    let pair = state.pair.clone();

    // asset should match either token
    ensure!(asset_in.exists(pair), ContractError::AssetDoesNotExist {});

    let swap_response = simulate_swap(deps, &STATE, &BALANCES, asset_in, amount_in)?;

    let response = match next_swaps.split_first() {
        Some((next_swap, forward_swaps)) => {
            let next_swap_response: GetSwapQueryResponse = deps.querier.query_wasm_smart(
                next_swap.vlp_address.clone(),
                &euclid::msgs::vlp::cp::msg::QueryMsg::SimulateSwap(VlpSimulateSwapMsg {
                    asset: swap_response.asset_out,
                    asset_amount: swap_response.amount_out,
                    swaps: forward_swaps.to_vec(),
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
        pool_config: PoolConfig::ConstantProduct {},
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

            Ok::<PoolInfo, ContractError>(PoolInfo { chain_uid, pool })
        })
        .collect();

    Ok(to_json_binary(&AllPoolsResponse { pools: pools? })?)
}

fn get_pool(
    state: &State,
    chain_lp_tokens: Uint256,
    reserve_1: Uint256,
    reserve_2: Uint256,
) -> Result<PoolResponse, ContractError> {
    Ok(PoolResponse {
        reserve_1: calculate_amount_from_shares(reserve_1, chain_lp_tokens, state.total_lp_tokens)
            .unwrap_or(Uint256::zero()),
        reserve_2: calculate_amount_from_shares(reserve_2, chain_lp_tokens, state.total_lp_tokens)
            .unwrap_or(Uint256::zero()),
        lp_shares: chain_lp_tokens,
    })
}

/// Extracts the token amount for a given token from a pair with amounts
/// by matching it against a reference token
pub fn extract_token_amount(liquidity: &PairWithAmount, pair: &Pair) -> (Uint256, Uint256) {
    let token_1_liquidity = if liquidity.token_1.token == pair.token_1 {
        liquidity.token_1.amount
    } else {
        liquidity.token_2.amount
    };

    let token_2_liquidity = if liquidity.token_2.token == pair.token_2 {
        liquidity.token_2.amount
    } else {
        liquidity.token_1.amount
    };

    (token_1_liquidity, token_2_liquidity)
}

#[cfg(test)]
mod tests {
    use crate::contract::{execute, query};
    use crate::query::query_simulate_swap;
    use crate::state::{ADMIN, BALANCES, STATE};
    use crate::testing::helpers::{
        default_admin, default_fee, default_pair, init, make_pair_with_amount, token1,
        TEST_VIRTUAL_BALANCE,
    };
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{from_json, Addr, Uint256};
    use euclid::admin::EuclidAdmin;
    use euclid::chain::ChainUid;
    use euclid::cross_chain_user::CrossChainUser;
    use euclid::error::ContractError;
    use euclid::fee::{DenomFees, Fee, TotalFees};
    use euclid::msgs::vlp::base::{
        GetLiquidityQueryResponse, GetSwapQueryResponse, State, VlpAddLiquidityMsg,
        VlpRegisterPoolMsg, VlpSwapMsg,
    };
    use euclid::msgs::vlp::cp::msg::{
        AllPoolsResponse, ExecuteMsg, FeeResponse, GetStateResponse, PoolResponse,
        TotalFeesPerDenomResponse, TotalFeesResponse,
    };
    use euclid::token::{Pair, Token};
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Query: State
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_state() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res: GetStateResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::State {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res.pair, default_pair());
        assert_eq!(res.total_lp_tokens, Uint256::zero());
        assert_eq!(res.last_updated, 0);
    }

    // -----------------------------------------------------------------------
    // Query: GetAdmin
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_admin() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res: EuclidAdmin = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::GetAdmin {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res, default_admin(&deps));
    }

    // -----------------------------------------------------------------------
    // Query: Fee
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_fee() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res: FeeResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::Fee {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res.fee, default_fee(&deps));
    }

    // -----------------------------------------------------------------------
    // Query: TotalFeesCollected
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_total_fees_collected_initially_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res: TotalFeesResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::TotalFeesCollected {},
            )
            .unwrap(),
        )
        .unwrap();

        assert!(res.total_fees.lp_fees.totals.is_empty());
        assert!(res.total_fees.euclid_fees.totals.is_empty());
    }

    #[test]
    fn test_query_total_fees_after_swap() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: sender.clone(),
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::Swap(VlpSwapMsg {
                sender: sender.clone(),
                tx_id: "swap1".to_string(),
                asset_in: token1(),
                amount_in: Uint256::from(10_000u128),
                min_token_out: Uint256::zero(),
                next_swaps: vec![],
                test_fail: None,
            }),
        )
        .unwrap();

        let res: TotalFeesResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::TotalFeesCollected {},
            )
            .unwrap(),
        )
        .unwrap();

        let lp_fee = res
            .total_fees
            .lp_fees
            .get_fee(token1().to_string().as_str());
        assert!(!lp_fee.is_zero());
    }

    // -----------------------------------------------------------------------
    // Query: TotalFeesPerDenom
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_total_fees_per_denom_zero_initially() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res: TotalFeesPerDenomResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::TotalFeesPerDenom {
                    denom: token1().to_string(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res.lp_fees, Uint256::zero());
        assert_eq!(res.euclid_fees, Uint256::zero());
    }

    // -----------------------------------------------------------------------
    // Query: Liquidity
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_liquidity_initially_zero() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res: GetLiquidityQueryResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::Liquidity {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res.token_1_reserve, Uint256::zero());
        assert_eq!(res.token_2_reserve, Uint256::zero());
        assert_eq!(res.total_lp_tokens, Uint256::zero());
        assert_eq!(res.pair, default_pair());
    }

    #[test]
    fn test_query_liquidity_after_add() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid, "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender,
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(5_000_000_000, 5_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let res: GetLiquidityQueryResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::Liquidity {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res.token_1_reserve, Uint256::from(5_000_000_000u128));
        assert_eq!(res.token_2_reserve, Uint256::from(5_000_000_000u128));
        assert!(!res.total_lp_tokens.is_zero());
    }

    // -----------------------------------------------------------------------
    // Query: Pool (per chain)
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
        let sender = CrossChainUser::new(chain_uid.clone(), "user1".to_string());
        let info = message_info(&router, &[]);

        execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                sender: sender.clone(),
                pair: default_pair(),
                tx_id: "reg".to_string(),
            }),
        )
        .unwrap();
        execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender,
                tx_id: "add".to_string(),
                liquidity: make_pair_with_amount(10_000_000_000, 10_000_000_000),
                slippage_tolerance_bps: 0,
            }),
        )
        .unwrap();

        let pool: PoolResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::Pool {
                    chain_uid: chain_uid.clone(),
                },
            )
            .unwrap(),
        )
        .unwrap();

        assert!(!pool.lp_shares.is_zero());
        assert!(!pool.reserve_1.is_zero());
        assert!(!pool.reserve_2.is_zero());
    }

    #[test]
    fn test_query_pool_not_found() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let missing_chain = ChainUid::create("missingchain".to_string()).unwrap();
        let res = query(
            deps.as_ref(),
            mock_env(),
            euclid::msgs::vlp::cp::msg::QueryMsg::Pool {
                chain_uid: missing_chain,
            },
        );
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // Query: GetAllPools
    // -----------------------------------------------------------------------

    #[test]
    fn test_query_all_pools_empty() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let res: AllPoolsResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::GetAllPools {},
            )
            .unwrap(),
        )
        .unwrap();

        assert!(res.pools.is_empty());
    }

    #[test]
    fn test_query_all_pools_multiple() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        init(&mut deps);

        let router = deps.api.addr_make("router");
        let info = message_info(&router, &[]);

        for chain_name in &["chain1", "chain2", "chain3"] {
            let chain_uid = ChainUid::create(chain_name.to_string()).unwrap();
            let sender = CrossChainUser::new(chain_uid.clone(), "user".to_string());
            execute(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                ExecuteMsg::RegisterPool(VlpRegisterPoolMsg {
                    sender,
                    pair: default_pair(),
                    tx_id: format!("reg_{chain_name}"),
                }),
            )
            .unwrap();
        }

        let res: AllPoolsResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                euclid::msgs::vlp::cp::msg::QueryMsg::GetAllPools {},
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(res.pools.len(), 3);
    }

    // -----------------------------------------------------------------------
    // Query: SimulateSwap
    // -----------------------------------------------------------------------

    #[test]
    fn test_simulate_swap_with_spread() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let pair = Pair {
            token_1: Token::create("uatom".to_string()).unwrap(),
            token_2: Token::create("uosmo".to_string()).unwrap(),
        };

        let state = State {
            pair: pair.clone(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
            fee: Fee::new(
                30,
                0,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
            total_fees_collected: TotalFees {
                lp_fees: DenomFees {
                    totals: HashMap::default(),
                },
                euclid_fees: DenomFees {
                    totals: HashMap::default(),
                },
            },
            last_updated: env.block.time.seconds(),
            total_lp_tokens: Uint256::from(100000u128),
        };

        let admin = EuclidAdmin::default(deps.api.addr_make("admin"));
        STATE.save(deps.as_mut().storage, &state).unwrap();
        ADMIN.save(deps.as_mut().storage, &admin).unwrap();

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

        let swap_amount = Uint256::from(100u128);
        let response: GetSwapQueryResponse = from_json(
            query_simulate_swap(deps.as_ref(), pair.token_1, swap_amount, vec![]).unwrap(),
        )
        .unwrap();

        assert_eq!(response.asset_out, pair.token_2);
        assert_eq!(response.amount_out, Uint256::from(46u128));
        assert_eq!(response.spread_amount, Uint256::from(4u128));
    }

    #[test]
    fn test_simulate_swap_zero_amount() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let err =
            query_simulate_swap(deps.as_ref(), token1(), Uint256::zero(), vec![]).unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_simulate_swap_unknown_token() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let unknown = Token::create("unknowntoken".to_string()).unwrap();
        let err = query_simulate_swap(deps.as_ref(), unknown, Uint256::from(100u128), vec![])
            .unwrap_err();
        assert_eq!(err, ContractError::AssetDoesNotExist {});
    }

    #[test]
    fn test_swap_with_large_reserve_ratio() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let pair = Pair {
            token_1: Token::create("uatom".to_string()).unwrap(),
            token_2: Token::create("uosmo".to_string()).unwrap(),
        };

        let state = State {
            pair: pair.clone(),
            router: Addr::unchecked("router"),
            virtual_balance_contract: Addr::unchecked(TEST_VIRTUAL_BALANCE),
            fee: Fee::new(
                30,
                0,
                CrossChainUser::new(
                    ChainUid::create("1".to_string()).unwrap(),
                    "addr".to_string(),
                ),
            ),
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

        let swap_amount = Uint256::from(10000000000000000u128);
        let response: GetSwapQueryResponse = from_json(
            query_simulate_swap(deps.as_ref(), pair.token_1, swap_amount, vec![]).unwrap(),
        )
        .unwrap();

        assert_eq!(response.asset_out, pair.token_2);
        assert_eq!(
            response.amount_out,
            Uint256::from(64696251029190591u128),
            "Amount out is not correct"
        );
        assert_eq!(
            response.spread_amount,
            Uint256::from(64687854381176u128),
            "Spread amount is not correct"
        );
    }
}
