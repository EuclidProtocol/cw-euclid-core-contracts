// #[cfg(test)]
// mod tests {
//     use crate::{
//         contract,
//         state::{State, STATE},
//     };

//     use super::*;
//     use cosmwasm_std::{Addr, Coin, Empty, Uint128};
//     use cw_multi_test::{App, AppBuilder, Contract, ContractWrapper, Executor};
//     use euclid::{
//         fee::Fee,
//         msgs::vlp::{
//             AllPoolsResponse, ExecuteMsg, FeeResponse, GetLiquidityResponse, GetSwapResponse,
//             InstantiateMsg, PoolResponse, QueryMsg,
//         },
//         token::{Pair, PairInfo, Token, TokenInfo, TokenType},
//     };

//     const USER: &str = "user";
//     const NATIVE_DENOM: &str = "native";
//     const IBC_DENOM_1: &str = "ibc/denom1";
//     const IBC_DENOM_2: &str = "ibc/denom2";
//     const SUPPLY: u128 = 1_000_000;

//     fn contract() -> Box<dyn Contract<Empty>> {
//         Box::new(
//             ContractWrapper::new_with_empty(
//                 crate::contract::execute,
//                 crate::contract::instantiate,
//                 crate::contract::query,
//             )
//             .with_reply_empty(contract::reply),
//         )
//     }

//     fn mock_app() -> App {
//         AppBuilder::new().build(|router, _, storage| {
//             router
//                 .bank
//                 .init_balance(
//                     storage,
//                     &Addr::unchecked(USER),
//                     vec![
//                         Coin {
//                             denom: NATIVE_DENOM.to_string(),
//                             amount: Uint128::from(SUPPLY),
//                         },
//                         Coin {
//                             denom: IBC_DENOM_1.to_string(),
//                             amount: Uint128::from(SUPPLY),
//                         },
//                         Coin {
//                             denom: IBC_DENOM_2.to_string(),
//                             amount: Uint128::from(SUPPLY),
//                         },
//                     ],
//                 )
//                 .unwrap();
//         })
//     }

//     #[test]
//     fn test_instantiate_contract() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Query the state to verify the instantiation
//         let res: GetLiquidityResponse = app
//             .wrap()
//             .query_wasm_smart(contract_addr.clone(), &QueryMsg::Liquidity {})
//             .unwrap();

//         assert_eq!(res.pair.token_1.id, "token_1_address".to_string());
//         assert_eq!(res.pair.token_2.id, "token_2_address".to_string());
//         assert_eq!(res.token_1_reserve, Uint128::zero());
//         assert_eq!(res.token_2_reserve, Uint128::zero());
//         assert_eq!(res.total_lp_tokens, Uint128::zero());
//     }

//     #[test]
//     fn test_register_pool() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Register a new pool
//         let register_msg = ExecuteMsg::RegisterPool {
//             chain_id: "chain_1".to_string(),
//             pair_info: PairInfo {
//                 token_1: TokenInfo {
//                     token: Token {
//                         id: "token_1_address".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_1".to_string(),
//                     },
//                 },
//                 token_2: TokenInfo {
//                     token: Token {
//                         id: "token_2_address".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_2".to_string(),
//                     },
//                 },
//             },
//         };

//         let _res = app
//             .execute_contract(owner.clone(), contract_addr.clone(), &register_msg, &[])
//             .unwrap();

//         // Query the pool to verify registration
//         let res: PoolResponse = app
//             .wrap()
//             .query_wasm_smart(
//                 contract_addr.clone(),
//                 &QueryMsg::Pool {
//                     chain_id: "chain_1".to_string(),
//                 },
//             )
//             .unwrap();

//         assert_eq!(res.pool.chain, "chain_1".to_string());
//         assert_eq!(
//             res.pool.pair.token_1.token.id,
//             "token_1_address".to_string()
//         );
//         assert_eq!(
//             res.pool.pair.token_2.token.id,
//             "token_2_address".to_string()
//         );
//     }

//     #[test]
//     fn test_add_liquidity() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Register a new pool
//         let register_msg = ExecuteMsg::RegisterPool {
//             chain_id: "chain_1".to_string(),
//             pair_info: PairInfo {
//                 token_1: TokenInfo {
//                     token: Token {
//                         id: "token_1_address".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_1".to_string(),
//                     },
//                 },
//                 token_2: TokenInfo {
//                     token: Token {
//                         id: "token_2_address".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_2".to_string(),
//                     },
//                 },
//             },
//         };

//         let _res = app
//             .execute_contract(owner.clone(), contract_addr.clone(), &register_msg, &[])
//             .unwrap();

//         // Add liquidity to the pool
//         let add_liquidity_msg = ExecuteMsg::AddLiquidity {
//             chain_id: "chain_1".to_string(),
//             token_1_liquidity: Uint128::new(500),
//             token_2_liquidity: Uint128::new(1000),
//             slippage_tolerance: 5,
//         };

//         let _res = app
//             .execute_contract(
//                 owner.clone(),
//                 contract_addr.clone(),
//                 &add_liquidity_msg,
//                 &[],
//             )
//             .unwrap();

//         // Query the pool to verify the added liquidity
//         let res: PoolResponse = app
//             .wrap()
//             .query_wasm_smart(
//                 contract_addr.clone(),
//                 &QueryMsg::Pool {
//                     chain_id: "chain_1".to_string(),
//                 },
//             )
//             .unwrap();

//         assert_eq!(res.pool.chain, "chain_1".to_string());
//         assert_eq!(res.pool.reserve_1, Uint128::new(500));
//         assert_eq!(res.pool.reserve_2, Uint128::new(1000));
//     }
//     #[test]
//     fn test_remove_liquidity() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Register a new pool
//         let register_msg = ExecuteMsg::RegisterPool {
//             chain_id: "chain_1".to_string(),
//             pair_info: PairInfo {
//                 token_1: TokenInfo {
//                     token: Token {
//                         id: "token_1".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_1".to_string(),
//                     },
//                 },
//                 token_2: TokenInfo {
//                     token: Token {
//                         id: "token_2".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_2".to_string(),
//                     },
//                 },
//             },
//         };

//         let _res = app
//             .execute_contract(owner.clone(), contract_addr.clone(), &register_msg, &[])
//             .unwrap();

//         // // Initialize state to ensure it starts with zero reserves
//         // let init_state = State {
//         //     router: "router".to_string(),
//         //     vcoin: "vcoin".to_string(),
//         //     last_updated: 0,
//         //     pair: Pair {
//         //         token_1: Token {
//         //             id: "token_1".to_string(),
//         //         },
//         //         token_2: Token {
//         //             id: "token_2".to_string(),
//         //         },
//         //     },
//         //     total_reserve_1: Uint128::new(10000),
//         //     total_reserve_2: Uint128::new(10000),
//         //     total_lp_tokens: Uint128::new(3000),
//         //     fee: Fee {
//         //         lp_fee: 1,
//         //         treasury_fee: 1,
//         //         staker_fee: 1,
//         //     },
//         // };
//         // STATE.save(&mut app.wrap(), &init_state).unwrap();

//         // Add liquidity to the pool to set up initial state
//         let add_liquidity_msg = ExecuteMsg::AddLiquidity {
//             chain_id: "chain_1".to_string(),
//             token_1_liquidity: Uint128::new(500),
//             token_2_liquidity: Uint128::new(1000),
//             slippage_tolerance: 5,
//         };

//         let _res = app
//             .execute_contract(
//                 owner.clone(),
//                 contract_addr.clone(),
//                 &add_liquidity_msg,
//                 &[],
//             )
//             .unwrap();

//         // Now remove liquidity from the pool
//         let remove_liquidity_msg = ExecuteMsg::RemoveLiquidity {
//             chain_id: "chain_1".to_string(),
//             lp_allocation: Uint128::new(50),
//         };

//         let _res = app
//             .execute_contract(
//                 owner.clone(),
//                 contract_addr.clone(),
//                 &remove_liquidity_msg,
//                 &[],
//             )
//             .unwrap();

//         // Query the pool to verify the removed liquidity
//         let res: PoolResponse = app
//             .wrap()
//             .query_wasm_smart(
//                 contract_addr.clone(),
//                 &QueryMsg::Pool {
//                     chain_id: "chain_1".to_string(),
//                 },
//             )
//             .unwrap();

//         assert_eq!(res.pool.chain, "chain_1".to_string());
//         assert_eq!(res.pool.reserve_1, Uint128::new(250)); // 500 - 250 (50% of 500)
//         assert_eq!(res.pool.reserve_2, Uint128::new(500)); // 1000 - 500 (50% of 1000)
//     }
//     #[test]
//     fn test_execute_swap() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         // Register the contract code
//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Register a new pool
//         let register_msg = ExecuteMsg::RegisterPool {
//             chain_id: "chain_1".to_string(),
//             pair_info: PairInfo {
//                 token_1: TokenInfo {
//                     token: Token {
//                         id: "token_1".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_1".to_string(),
//                     },
//                 },
//                 token_2: TokenInfo {
//                     token: Token {
//                         id: "token_2".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_2".to_string(),
//                     },
//                 },
//             },
//         };

//         let _res = app
//             .execute_contract(owner.clone(), contract_addr.clone(), &register_msg, &[])
//             .unwrap();

//         // // Initialize state to ensure it starts with zero reserves
//         // let init_state = State {
//         //     router: "router".to_string(),
//         //     vcoin: "vcoin".to_string(),
//         //     last_updated: 0,
//         //     pair: Pair {
//         //         token_1: Token {
//         //             id: "token_1".to_string(),
//         //         },
//         //         token_2: Token {
//         //             id: "token_2".to_string(),
//         //         },
//         //     },
//         //     total_reserve_1: Uint128::new(10000),
//         //     total_reserve_2: Uint128::new(10000),
//         //     total_lp_tokens: Uint128::new(3000),
//         //     fee: Fee {
//         //         lp_fee: 1,
//         //         treasury_fee: 1,
//         //         staker_fee: 1,
//         //     },
//         // };
//         // STATE.save(&mut app.wrap(), &init_state).unwrap();

//         // Add liquidity to the pool to set up initial state
//         let add_liquidity_msg = ExecuteMsg::AddLiquidity {
//             chain_id: "chain_1".to_string(),
//             token_1_liquidity: Uint128::new(500),
//             token_2_liquidity: Uint128::new(1000),
//             slippage_tolerance: 5,
//         };

//         let _res = app
//             .execute_contract(
//                 owner.clone(),
//                 contract_addr.clone(),
//                 &add_liquidity_msg,
//                 &[],
//             )
//             .unwrap();

//         // Execute a swap
//         let execute_swap_msg = ExecuteMsg::Swap {
//             to_chain_id: "chain_1".to_string(),
//             to_address: "recipient_address".to_string(),
//             asset_in: Token {
//                 id: "token_1".to_string(),
//             },
//             amount_in: Uint128::new(100),
//             min_token_out: Uint128::new(50),
//             swap_id: "swap_id".to_string(),
//             next_swaps: vec![],
//         };

//         let _res = app
//             .execute_contract(owner.clone(), contract_addr.clone(), &execute_swap_msg, &[])
//             .unwrap();

//         // Query the pool to verify the swap
//         let res: PoolResponse = app
//             .wrap()
//             .query_wasm_smart(
//                 contract_addr.clone(),
//                 &QueryMsg::Pool {
//                     chain_id: "chain_1".to_string(),
//                 },
//             )
//             .unwrap();

//         // Verify the updated pool reserves
//         assert_eq!(res.pool.chain, "chain_1".to_string());
//         assert!(res.pool.reserve_1 < Uint128::new(500)); // token_1 reserve should decrease
//         assert!(res.pool.reserve_2 > Uint128::new(1000)); // token_2 reserve should increase
//     }
//     #[test]
//     fn test_query_simulate_swap() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Add liquidity to set up initial state
//         let add_liquidity_msg = ExecuteMsg::AddLiquidity {
//             chain_id: "chain_1".to_string(),
//             token_1_liquidity: Uint128::new(500),
//             token_2_liquidity: Uint128::new(1000),
//             slippage_tolerance: 5,
//         };

//         app.execute_contract(
//             owner.clone(),
//             contract_addr.clone(),
//             &add_liquidity_msg,
//             &[],
//         )
//         .unwrap();

//         // Simulate a swap
//         let simulate_swap_msg = QueryMsg::SimulateSwap {
//             asset: Token {
//                 id: "token_1".to_string(),
//             },
//             asset_amount: Uint128::new(100),
//             swaps: vec![],
//         };

//         let res: GetSwapResponse = app
//             .wrap()
//             .query_wasm_smart(contract_addr.clone(), &simulate_swap_msg)
//             .unwrap();

//         assert!(res.amount_out > Uint128::zero());
//     }

//     #[test]
//     fn test_query_liquidity() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Add liquidity to set up initial state
//         let add_liquidity_msg = ExecuteMsg::AddLiquidity {
//             chain_id: "chain_1".to_string(),
//             token_1_liquidity: Uint128::new(500),
//             token_2_liquidity: Uint128::new(1000),
//             slippage_tolerance: 5,
//         };

//         app.execute_contract(
//             owner.clone(),
//             contract_addr.clone(),
//             &add_liquidity_msg,
//             &[],
//         )
//         .unwrap();

//         // Query liquidity
//         let query_liquidity_msg = QueryMsg::Liquidity {};

//         let res: GetLiquidityResponse = app
//             .wrap()
//             .query_wasm_smart(contract_addr.clone(), &query_liquidity_msg)
//             .unwrap();

//         assert_eq!(res.token_1_reserve, Uint128::new(500));
//         assert_eq!(res.token_2_reserve, Uint128::new(1000));
//         assert_eq!(res.total_lp_tokens, Uint128::new(250)); // Assuming 1:1 ratio and initial liquidity
//     }

//     #[test]
//     fn test_query_fee() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Query fee
//         let query_fee_msg = QueryMsg::Fee {};

//         let res: FeeResponse = app
//             .wrap()
//             .query_wasm_smart(contract_addr.clone(), &query_fee_msg)
//             .unwrap();

//         assert_eq!(res.fee.lp_fee, 1);
//         assert_eq!(res.fee.treasury_fee, 1);
//         assert_eq!(res.fee.staker_fee, 1);
//     }

//     #[test]
//     fn test_query_pool() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Register a new pool
//         let register_msg = ExecuteMsg::RegisterPool {
//             chain_id: "chain_1".to_string(),
//             pair_info: PairInfo {
//                 token_1: TokenInfo {
//                     token: Token {
//                         id: "token_1".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_1".to_string(),
//                     },
//                 },
//                 token_2: TokenInfo {
//                     token: Token {
//                         id: "token_2".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_2".to_string(),
//                     },
//                 },
//             },
//         };

//         app.execute_contract(owner.clone(), contract_addr.clone(), &register_msg, &[])
//             .unwrap();

//         // Query pool
//         let query_pool_msg = QueryMsg::Pool {
//             chain_id: "chain_1".to_string(),
//         };
//         let res: PoolResponse = app
//             .wrap()
//             .query_wasm_smart(contract_addr.clone(), &query_pool_msg)
//             .unwrap();

//         assert_eq!(res.pool.chain, "chain_1".to_string());
//         assert_eq!(res.pool.reserve_1, Uint128::zero());
//         assert_eq!(res.pool.reserve_2, Uint128::zero());
//     }

//     #[test]
//     fn test_query_all_pools() {
//         let mut app = mock_app();
//         let owner = Addr::unchecked("owner");

//         let code_id = app.store_code(contract());

//         let instantiate_msg = InstantiateMsg {
//             router: "router_address".to_string(),
//             vcoin: "vcoin_address".to_string(),
//             pair: Pair {
//                 token_1: Token {
//                     id: "token_1_address".to_string(),
//                 },
//                 token_2: Token {
//                     id: "token_2_address".to_string(),
//                 },
//             },
//             fee: Fee {
//                 lp_fee: 1,
//                 treasury_fee: 1,
//                 staker_fee: 1,
//             },
//             execute: None,
//         };

//         let contract_addr = app
//             .instantiate_contract(
//                 code_id,
//                 owner.clone(),
//                 &instantiate_msg,
//                 &[],
//                 "Contract",
//                 None,
//             )
//             .unwrap();

//         // Register a new pool
//         let register_msg = ExecuteMsg::RegisterPool {
//             chain_id: "chain_1".to_string(),
//             pair_info: PairInfo {
//                 token_1: TokenInfo {
//                     token: Token {
//                         id: "token_1".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_1".to_string(),
//                     },
//                 },
//                 token_2: TokenInfo {
//                     token: Token {
//                         id: "token_2".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_2".to_string(),
//                     },
//                 },
//             },
//         };

//         app.execute_contract(owner.clone(), contract_addr.clone(), &register_msg, &[])
//             .unwrap();

//         // Register another pool
//         let register_msg = ExecuteMsg::RegisterPool {
//             chain_id: "chain_2".to_string(),
//             pair_info: PairInfo {
//                 token_1: TokenInfo {
//                     token: Token {
//                         id: "token_1".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_1".to_string(),
//                     },
//                 },
//                 token_2: TokenInfo {
//                     token: Token {
//                         id: "token_2".to_string(),
//                     },
//                     token_type: TokenType::Native {
//                         denom: "token_2".to_string(),
//                     },
//                 },
//             },
//         };

//         app.execute_contract(owner.clone(), contract_addr.clone(), &register_msg, &[])
//             .unwrap();

//         // Query all pools
//         let query_all_pools_msg = QueryMsg::GetAllPools {};

//         let res: AllPoolsResponse = app
//             .wrap()
//             .query_wasm_smart(contract_addr.clone(), &query_all_pools_msg)
//             .unwrap();

//         assert_eq!(res.pools.len(), 2);
//     }
// }
