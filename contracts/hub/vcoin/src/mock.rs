#!
use crate::contract::{execute, instantiate, query};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::{mock::MockApp, msgs::vcoin::InstantiateMsg};

pub struct MockVcoin(Addr);

impl MockVcoin {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        router: Addr,
        admin: Option<Addr>,
    ) -> Self {
        let msg = mock_vcoin_instantiate_msg(router, admin);
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid vcoin", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_vcoin_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }
}

pub fn mock_vcoin() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query);
    Box::new(contract)
}

pub fn mock_vcoin_instantiate_msg(router: Addr, admin: Option<Addr>) -> InstantiateMsg {
    InstantiateMsg { router, admin }
}

// pub fn mock_vcoin_send_msg() -> ExecuteMsg {

// }
