#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::msgs::pool_factory::InstantiateMsg;
use mock::mock::MockApp;

pub struct MockPoolFactory(Addr);
impl MockPoolFactory {
    pub fn addr(&self) -> &Addr {
        &self.0
    }
}

impl MockPoolFactory {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        main_factory_address: String,
    ) -> Self {
        let msg = InstantiateMsg {
            main_factory_address,
        };
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid pool factory", None);
        Self(res.unwrap())
    }
}

pub fn mock_pool_factory() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}
