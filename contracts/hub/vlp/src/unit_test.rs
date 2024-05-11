#[cfg(test)]
mod tests {

    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins,    Response, Uint128};
    use euclid::fee::Fee;
    use euclid::pool::Pool;
    use euclid::error::ContractError;
  
    use crate::state::{ STATE, POOLS};


    use euclid::token::{Pair, PairInfo, Token, TokenInfo};
    use crate::contract::{instantiate,execute,query};
    use crate::msg::{ InstantiateMsg,ExecuteMsg, QueryMsg};



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
}