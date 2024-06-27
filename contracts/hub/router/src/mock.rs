#![cfg(all(not(target_arch = "wasm32")))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Coin, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::msgs::router::{ExecuteMsg, InstantiateMsg, QueryMsg};
use euclid::testing::{ExecuteResult, MockApp};

pub struct MockRouter(Addr);

impl MockRouter {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        owner: Option<String>,
        vlp_code_id: u64,
        vcoin_code_id: u64,
    ) -> Self {
        let msg = mock_router_instantiate_msg(vlp_code_id, vcoin_code_id, owner);
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid router", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_router_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }
}

pub fn mock_router() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_router_instantiate_msg(
    vlp_code_id: u64,
    vcoin_code_id: u64,
    owner: Option<String>,
) -> InstantiateMsg {
    InstantiateMsg {
        vlp_code_id,
        vcoin_code_id,
    }
}

// pub fn mock_router_send_msg() -> ExecuteMsg {

// }
