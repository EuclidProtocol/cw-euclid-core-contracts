// use crate::contract::{execute, instantiate, query};
// use cw_orch::{interface, prelude::*};
// use euclid::msgs::cw20::{ExecuteMsg, InstantiateMsg, QueryMsg};
// pub const CONTRACT_ID: &str = "cw20_contract";

// #[interface(InstantiateMsg, ExecuteMsg, QueryMsg, Empty, id = CONTRACT_ID)]
// pub struct Cw20Contract<Chain: CwEnv>;

// // Implement the Uploadable trait so it can be uploaded to the mock.
// impl<Chain> Uploadable for Cw20Contract<Chain> {
//     fn wrapper() -> Box<dyn MockContract<Empty>> {
//         Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
//     }
// }
