#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query};
use cosmwasm_std::Empty;
use cw_multi_test::{Contract, ContractWrapper};

pub fn mock_position_token() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query);
    Box::new(contract)
}
