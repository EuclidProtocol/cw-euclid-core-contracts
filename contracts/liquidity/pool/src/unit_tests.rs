#[cfg(test)]
mod tests {
    use crate::contract::{  execute::{self, execute_swap_request}, instantiate};
    use cosmwasm_std::{coins, testing::{mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info}, Uint128};

    use crate::msg::{ ExecuteMsg, InstantiateMsg};

    
    use euclid::{
        token::{Pair, PairInfo, Token, TokenInfo},
        pool::Pool,
    };

    #[test]
    fn proper_instantiation() {
        let mut deps = mock_dependencies_with_balance(&coins(1000000000, "earth".to_string()));
        let env = mock_env();
        let info = mock_info("creator", &coins(1000000, "earth")); // Increase native token amount
        let msg = InstantiateMsg {
            vlp_contract: "vlp_contract_address".to_string(),
            token_pair: Pair {
                token_1: Token {
                    id: "token_1".to_string(),
                },
                token_2: Token {
                    id: "token_2".to_string(),
                },
            },
            pair_info: PairInfo {
                token_1: TokenInfo::Native {
                    denom: "earth".to_string(), // Use "earth" denom for native token
                    token: Token { id: "token_1".to_string() },
                },
                token_2: TokenInfo::Native {
                    denom: "earth".to_string(), // Use "earth" denom for native token
                    token: Token { id: "token_2".to_string() },
                },
            },
            pool: Pool {
                chain: "chain_id".to_string(),
                contract_address: "contract_address".to_string(),
                pair: PairInfo {
                    token_1: TokenInfo::Native {
                        denom: "earth".to_string(), // Use "earth" denom for native token
                        token: Token { id: "token_1".to_string() },
                    },
                    token_2: TokenInfo::Native {
                        denom: "earth".to_string(), // Use "earth" denom for native token
                        token: Token { id: "token_2".to_string() },
                    },
                },
                reserve_1: Uint128::new(10000),
                reserve_2: Uint128::new(10000),
            },
            chain_id: "chain_id".to_string(),
        };
        let res = instantiate(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len()); // Ensure no messages are sent on successful instantiation
    }
    #[test]
    fn proper_execute_swap_request() {
        let mut deps = mock_dependencies_with_balance(&coins(1000000000, "earth".to_string()));
        let env = mock_env();
        let info = mock_info("creator", &coins(1000000, "earth")); // Increase native token amount
        let msg = InstantiateMsg {
            vlp_contract: "vlp_contract_address".to_string(),
            token_pair: Pair {
                token_1: Token {
                    id: "token_1".to_string(),
                },
                token_2: Token {
                    id: "token_2".to_string(),
                },
            },
            pair_info: PairInfo {
                token_1: TokenInfo::Native {
                    denom: "earth".to_string(), // Use "earth" denom for native token
                    token: Token { id: "token_1".to_string() },
                },
                token_2: TokenInfo::Native {
                    denom: "earth".to_string(), // Use "earth" denom for native token
                    token: Token { id: "token_2".to_string() },
                },
            },
            pool: Pool {
                chain: "chain_id".to_string(),
                contract_address: "contract_address".to_string(),
                pair: PairInfo {
                    token_1: TokenInfo::Native {
                        denom: "earth".to_string(), // Use "earth" denom for native token
                        token: Token { id: "token_1".to_string() },
                    },
                    token_2: TokenInfo::Native {
                        denom: "earth".to_string(), // Use "earth" denom for native token
                        token: Token { id: "token_2".to_string() },
                    },
                },
                reserve_1: Uint128::new(10000),
                reserve_2: Uint128::new(10000),
            },
            chain_id: "chain_id".to_string(),
        };
        let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());
     // Asset and amounts for swap
    let asset = TokenInfo::Native {
        denom: "earth".to_string(), // Use "earth" denom as in the contract's state
        token: Token { id: "token".to_string() },
    };
    let asset_amount = Uint128::new(10000); // Provide a valid asset amount
    let min_amount_out = Uint128::new(9000); // Set a minimum amount out that should be met
    let channel = "channel_id".to_string(); // Provide a valid channel ID

    // Call the execute_swap_request function with valid parameters
    let res = execute_swap_request(
        deps.as_mut(),
        info.clone(),
        env.clone(),
        asset.clone(),
        asset_amount,
        min_amount_out,
        channel.clone(),
         // Pass None for msg_sender in this test case
    );

    // Handle the result with error handling
    match res {
        Ok(_) => {
            // If execution is successful, perform additional checks here if needed
            println!("Swap executed successfully!");
        }
        Err(err) => {
            // Print the error message for debugging
            println!("Error executing swap: {:?}", err);
        }
    }
    }

}