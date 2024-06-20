#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};

    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{attr, from_binary, to_binary, Coin, Storage, Uint128};
    use euclid::msgs::factory::{ExecuteMsg, GetPoolResponse, InstantiateMsg, QueryMsg, StateResponse};
    use euclid::token::{PairInfo, Token, TokenInfo, TokenType};

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };

        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.attributes, vec![
            attr("method", "instantiate"),
            attr("router_contract", "router_contract"),
            attr("chain_id", "cosmos-testnet-14002"), // Adjust according to your mock environment
            attr("escrow_code_id", "1"),
        ]);
    }

    #[test]
    fn test_execute_request_add_allowed_denom() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let token = Token {
            id: "token_id".to_string(),
        };
        let denom = "denom".to_string();

        let msg = ExecuteMsg::RequestAddAllowedDenom { token_id: token.clone(), denom: denom.clone() };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();

        assert_eq!(res.attributes, vec![
            attr("method", "request_add_allowed_denom"),
            attr("token", token.id),
            attr("denom", denom),
        ]);
    }

    #[test]
    fn test_execute_request_deregister_denom() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);
    
        // Instantiate the contract
        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    
        
    
        // Set up mock escrow data if required
        let token_id = Token {
            id: "token_id".to_string(),
        };
        let denom = "denom".to_string();
    
        // Execute the deregister denom operation
        let msg = ExecuteMsg::RequestDeregisterDenom {
            token_id: token_id.clone(),
            denom: denom.clone(),
        };
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    
        // Assert the expected attributes in the response
        assert_eq!(
            res.attributes,
            vec![
                attr("method", "request_disallow_denom"),
                attr("token", token_id.id.clone()),
                attr("denom", denom.clone()),
            ]
        );
    }

 
    #[test]
    fn test_execute_request_pool_creation() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[]);

        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // Mocking the Hub Channel existence
        // store_hub_channel(deps.as_mut().storage, "hub_channel").unwrap();

        let pair_info = PairInfo {
            token_1: TokenInfo {
                token: Token {
                    id: "token_1".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_1".to_string(),
                },
            },
            token_2: TokenInfo {
                token: Token {
                    id: "token_2".to_string(),
                },
                token_type: TokenType::Native {
                    denom: "token_2".to_string(),
                },
            },
        };

        let msg = ExecuteMsg::RequestPoolCreation { pair_info: pair_info.clone(), timeout: None };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();

        assert_eq!(res.attributes, vec![
            attr("method", "request_pool_creation"),
        ]);
    }

    #[test]
    fn test_add_liquidity_request() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[Coin {
            denom: "token1".to_string(),
            amount: Uint128::new(100),
        }, Coin {
            denom: "token2".to_string(),
            amount: Uint128::new(100),
        }]);

        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::AddLiquidityRequest {
            vlp_address: "vlp_address".to_string(),
            token_1_liquidity: Uint128::new(100),
            token_2_liquidity: Uint128::new(100),
            slippage_tolerance: 1,
            timeout: None,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();

        assert_eq!(res.attributes, vec![
            attr("method", "add_liquidity_request"),
        ]);
    }

    #[test]
    fn test_execute_swap_request() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("creator", &[Coin {
            denom: "token1".to_string(),
            amount: Uint128::new(100),
        }]);

        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let asset_in =  TokenInfo {
            token: Token {
                id: "token_1".to_string(),
            },
            token_type: TokenType::Native {
                denom: "token_1".to_string(),
            },
        } ;
        let asset_out =  TokenInfo {
            token: Token {
                id: "token_2".to_string(),
            },
            token_type: TokenType::Native {
                denom: "token_2".to_string(),
            },
        };

        let msg = ExecuteMsg::ExecuteSwapRequest {
            asset_in: asset_in.clone(),
            asset_out: asset_out.clone(),
            amount_in: Uint128::new(100),
            min_amount_out: Uint128::new(90),
            timeout: None,
            swaps: vec![],
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();

        assert_eq!(res.attributes, vec![
            attr("method", "execute_request_swap"),
        ]);
    }

    #[test]
    fn test_query_get_pool() {
        let mut deps = mock_dependencies();
        let env = mock_env();
    
        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), mock_info("creator", &[]), msg).unwrap();
    
        // Ensure that any initial state setup needed for testing is correctly done here
        // For example, store a mock PairInfo
        let pair_info = PairInfo {
            token_1: TokenInfo {
                token: Token { id: "token1".to_string() },
                token_type: TokenType::Native { denom: "token1".to_string() },
            },
            token_2: TokenInfo {
                token: Token { id: "token2".to_string() },
                token_type: TokenType::Native { denom: "token2".to_string() },
            },
        };
        let key = b"pair_info_key";
        deps.storage.set(key, &to_binary(&pair_info).unwrap());
    
        let msg = QueryMsg::GetPool { vlp: "vlp_address".to_string() };
        let res: GetPoolResponse = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();
    
        // Assert the expected values from the query result
        assert_eq!(res.pair_info.token_1.get_denom(), "token1");
        assert_eq!(res.pair_info.token_2.get_denom(), "token2");
    }
    

    #[test]
    fn test_query_get_state() {
        let mut deps = mock_dependencies();
        let env = mock_env();
    
        let chain_id = "chain_id";  // Ensure this matches your expected chain_id
    
        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: chain_id.to_string(),  // Set the correct chain_id here
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), mock_info("creator", &[]), msg).unwrap();
    
        let msg = QueryMsg::GetState {};
        let res: StateResponse = from_binary(&query(deps.as_ref(), env.clone(), msg).unwrap()).unwrap();
        println!("Query result: {:?}", res);  // Print out the result to debug

    
        // Adjust according to your mock environment
        assert_eq!(res.router_contract, "router_contract");
        assert_eq!(res.chain_id, chain_id);  // Assert against the correct chain_id
    }
    #[test]
    fn test_query_get_all_pools() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let msg = InstantiateMsg {
            router_contract: "router_contract".to_string(),
            chain_id: "chain_id".to_string(),
            escrow_code_id: 1,
        };
        instantiate(deps.as_mut(), env.clone(), mock_info("creator", &[]), msg).unwrap();

        let msg = QueryMsg::GetAllPools {};
        let res: Vec<GetPoolResponse> = from_binary(&query(deps.as_ref(), env, msg).unwrap()).unwrap();

        // Adjust according to your mock environment
        assert!(res.len() > 0);
        assert_eq!(res[0].pair_info.token_1.get_denom(), "token1");
        assert_eq!(res[0].pair_info.token_2.get_denom(), "token2");
    }

}
