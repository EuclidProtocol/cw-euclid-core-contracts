#[cfg(test)]
mod tests {


    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary, from_json, Response, Uint128};
    use euclid::fee::Fee;
    use euclid::pool::Pool;
    use euclid::error::ContractError;
    use crate::contract::query::{query_liquidity, query_liquidity_info, query_simulate_swap};
    use crate::state::{State,  POOLS, STATE};


    use euclid::token::{Pair, PairInfo, Token, TokenInfo};
    use crate::contract::{  execute, instantiate};
    
    use crate::msg::{ ExecuteMsg, GetLiquidityResponse, GetSwapResponse, InstantiateMsg, LiquidityInfoResponse};



    #[test]
    // Write a test for instantiation
    fn proper_instantiation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &coins(1000, "earth"));
        let msg = InstantiateMsg {
            router: "router".to_string(),
            pair: Pair {
                token_1: Token {
                    id: "token_1".to_string(),
                },
                token_2: Token {
                    id: "token_2".to_string(),
                },
            },
            fee: Fee {
                lp_fee: 1,
                treasury_fee: 1,
                staker_fee: 1,
            },
            pool: Pool {
                chain: "chain".to_string(),
                contract_address: "contract_address".to_string(),
                pair: PairInfo {
                    token_1: TokenInfo::Native { denom: "token_1".to_string(),
                 token: Token { id: "token_1".to_string() }},

                    token_2: TokenInfo::Native { denom: "token_2".to_string(),
                    token: Token { id: "token_2".to_string()},
                    },
                },
                reserve_1: Uint128::new(10000),
                reserve_2: Uint128::new(10000),
            },
        };
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }
    #[test]
    fn test_execute_register_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &coins(1000, "earth"));
        let msg = InstantiateMsg {
            router: "router".to_string(),
            pair: Pair {
                token_1: Token {
                    id: "token_1".to_string(),
                },
                token_2: Token {
                    id: "token_2".to_string(),
                },
            },
            fee: Fee {
                lp_fee: 1,
                treasury_fee: 1,
                staker_fee: 1,
            },
            pool: Pool {
                chain: "chain".to_string(),
                contract_address: "contract_address".to_string(),
                pair: PairInfo {
                    token_1: TokenInfo::Native { denom: "token_1".to_string(),
                 token: Token { id: "token_1".to_string() }},

                    token_2: TokenInfo::Native { denom: "token_2".to_string(),
                    token: Token { id: "token_2".to_string()},
                    },
                },
                reserve_1: Uint128::new(10000),
                reserve_2: Uint128::new(10000),
            },
        };
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        let pool = Pool {
            chain: "chain_id".to_string(),
            contract_address: "contract_address".to_string(),
            pair: PairInfo {token_1: TokenInfo::Native { denom: "token_1".to_string(),
            token: Token { id: "token_1".to_string() }},
            token_2: TokenInfo::Native { denom: "token_2".to_string(),
                    token: Token { id: "token_2".to_string()},
                    },
                },

            
            reserve_1: Uint128::new(100),
            reserve_2: Uint128::new(200),
        };
        let msg = ExecuteMsg::RegisterPool {pool: pool.clone() };

        // Perform the execute and check the response
        let res: Response = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0); // Ensure no extra messages are sent

        // Verify that the pool was registered
        let pool_data = POOLS.load(deps.as_ref().storage, &pool.chain).unwrap();
        assert_eq!(pool_data, pool);
    }

    #[test]
fn register_pool_with_existing_pool_fails() {
    // Arrange
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("creator", &coins(1000, "earth"));
    let msg = InstantiateMsg {
        router: "router".to_string(),
        pair: Pair {
            token_1: Token {
                id: "token_1".to_string(),
            },
            token_2: Token {
                id: "token_2".to_string(),
            },
        },
        fee: Fee {
            lp_fee: 1,
            treasury_fee: 1,
            staker_fee: 1,
        },
        pool: Pool {
            chain: "chain".to_string(),
            contract_address: "contract_address".to_string(),
            pair: PairInfo {
                token_1: TokenInfo::Native { denom: "token_1".to_string(),
             token: Token { id: "token_1".to_string() }},

                token_2: TokenInfo::Native { denom: "token_2".to_string(),
                token: Token { id: "token_2".to_string()},
                },
            },
            reserve_1: Uint128::new(10000),
            reserve_2: Uint128::new(10000),
        },
    };
    let res = instantiate(deps.as_mut(),  env.clone(),info.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());
    let pool = Pool {
        chain: "chain_id".to_string(),
        contract_address: "contract_address".to_string(),
        pair: PairInfo {token_1: TokenInfo::Native { denom: "token_1".to_string(),
        token: Token { id: "token_1".to_string() }},
        token_2: TokenInfo::Native { denom: "token_2".to_string(),
                token: Token { id: "token_2".to_string()},
                },
            },

        
        reserve_1: Uint128::new(100),
        reserve_2: Uint128::new(200),
    };
    // Register the pool once (simulating an existing pool)
    execute::register_pool(deps.as_mut(), info.clone(), pool.clone()).unwrap();

    // Act: Attempt to register the same pool again
    let res = execute::register_pool(deps.as_mut(), info.clone(), pool.clone());

    // Assert: Check that registering the same pool again fails with the expected error
    match res {
        Err(ContractError::PoolAlreadyExists {}) => {} // This is the expected error case
        _ => panic!("Unexpected result: {:?}", res),
    }
}

#[test]
fn test_add_liquidity() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("lp_provider", &coins(1000, "token"));

    // Instantiate the contract
    let instantiate_msg = InstantiateMsg {
        router: "router".to_string(),
        pair: Pair {
            token_1: Token { id: "token_1".to_string() },
            token_2: Token { id: "token_2".to_string() },
        },
        fee: Fee {
            lp_fee: 1,
            treasury_fee: 1,
            staker_fee: 1,
        },
        pool: Pool {
            chain: "chain".to_string(),
            contract_address: "contract_address".to_string(),
            pair: PairInfo {
                token_1: TokenInfo::Native {
                    denom: "token_1".to_string(),
                    token: Token { id: "token_1".to_string() },
                },
                token_2: TokenInfo::Native {
                    denom: "token_2".to_string(),
                    token: Token { id: "token_2".to_string() },
                },
            },
            reserve_1: Uint128::new(10000),
            reserve_2: Uint128::new(10000),
        },
    };
    let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();

    // Define parameters for add liquidity
    let chain_id = "chain".to_string();
    let token_1_liquidity = Uint128::new(100);
    let token_2_liquidity = Uint128::new(100);
    let slippage_tolerance = 5; // Set a slippage tolerance within acceptable range

    // Execute add liquidity operation
    let msg = ExecuteMsg::AddLiquidity {
        chain_id: chain_id.clone(),
        token_1_liquidity,
        token_2_liquidity,
        slippage_tolerance,
        channel: 1.to_string(), // Include the required channel field
    };
    let res: Response = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 1); // Ensure no extra messages are sent

    // Verify that liquidity was added successfully
    let pool = POOLS.load(deps.as_ref().storage, &chain_id).unwrap();
    // Check if the ratio is maintained
    let expected_token_2_liquidity = token_1_liquidity * pool.reserve_2 / pool.reserve_1;
    assert_eq!(pool.reserve_1, Uint128::new(10000)); // Initial reserve + added liquidity
    assert_eq!(pool.reserve_2, Uint128::new(10100) - expected_token_2_liquidity); // Initial reserve + added liquidity - token 2 liquidity
}
#[test]
fn test_query_simulate_swap_valid_asset() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let asset = Token { id: "asset1".to_string() };
    let asset_amount = Uint128::new(1000);
    
    // Initialize contract state with valid asset info
    let  state = State {
        pair: Pair { token_1: Token { id: "asset1".to_string() }, token_2: Token { id: "asset2".to_string() } },
        router: "router1".to_string(),
        fee: Fee { lp_fee: 1, staker_fee: 1, treasury_fee: 1 },
        last_updated: 0,
        total_reserve_1: Uint128::new(10000),
        total_reserve_2: Uint128::new(20000),
        total_lp_tokens: Uint128::new(100000),
    };
    STATE.save(&mut deps.storage, &state).unwrap();

    // Call the query_simulate_swap function
    let result = query_simulate_swap(deps.as_ref(), asset.clone(), asset_amount).unwrap();
    let response: GetSwapResponse = from_json(&result).unwrap();

    // Assert that the received amount is as expected
    assert_eq!(response.token_out, Uint128::new(1769)); // Example calculation result
}
#[test]
fn test_query_simulate_swap_invalid_asset() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let asset = Token { id: "nonexistent_asset".to_string() };
    let asset_amount = Uint128::new(1000);
    
    // Initialize contract state with different asset info
    let state = State {
        pair: Pair { token_1: Token { id: "asset1".to_string() }, token_2: Token { id: "asset2".to_string() } },
        router: "router1".to_string(),
        fee: Fee { lp_fee: 1, staker_fee: 1, treasury_fee: 1 },
        last_updated: 0,
        total_reserve_1: Uint128::new(10000),
        total_reserve_2: Uint128::new(20000),
        total_lp_tokens: Uint128::new(100000),
    };
    STATE.save(&mut deps.storage, &state).unwrap();

    // Call the query_simulate_swap function with an invalid asset
    let result = query_simulate_swap(deps.as_ref(), asset.clone(), asset_amount);
    
    // Expect Err(ContractError::AssetDoesNotExist) due to invalid asset
    assert!(result.is_err());
}
#[test]
fn test_query_liquidity() {
    // Initialize mock dependencies and contract state
    let mut deps = mock_dependencies();
    let state = State {
        pair: Pair { token_1: Token { id: "asset1".to_string() }, token_2: Token { id: "asset2".to_string() } },
        router: "router1".to_string(),
        fee: Fee { lp_fee: 1, staker_fee: 1, treasury_fee: 1 },
        last_updated: 0,
        total_reserve_1: Uint128::new(10000),
        total_reserve_2: Uint128::new(20000),
        total_lp_tokens: Uint128::new(100000),
    };
    STATE.save(&mut deps.storage, &state).unwrap();

    // Call the query_liquidity function
    let result = query_liquidity(deps.as_ref()).unwrap();
    let response: GetLiquidityResponse = from_json(&result).unwrap();

    // Assert that the returned liquidity information matches the expected values
    assert_eq!(response.token_1_reserve, Uint128::new(10000));
    assert_eq!(response.token_2_reserve, Uint128::new(20000));
}
#[test]
fn test_query_liquidity_info() {
    // Initialize mock dependencies and contract state
    let mut deps = mock_dependencies();
    let state = State {
        pair: Pair { token_1: Token { id: "asset1".to_string() }, token_2: Token { id: "asset2".to_string() } },
        router: "router1".to_string(),
        fee: Fee { lp_fee: 1, staker_fee: 1, treasury_fee: 1 },
        last_updated: 0,
        total_reserve_1: Uint128::new(10000),
        total_reserve_2: Uint128::new(20000),
        total_lp_tokens: Uint128::new(100000),
    };
    STATE.save(&mut deps.storage, &state).unwrap();

    // Call the query_liquidity_info function
    let result = query_liquidity_info(deps.as_ref()).unwrap();
    let response: LiquidityInfoResponse = from_json(&result).unwrap();

    // Assert that the returned liquidity info with pair matches the expected values
    assert_eq!(response.pair, state.pair);
    assert_eq!(response.token_1_reserve, Uint128::new(10000));
    assert_eq!(response.token_2_reserve, Uint128::new(20000));
}
}