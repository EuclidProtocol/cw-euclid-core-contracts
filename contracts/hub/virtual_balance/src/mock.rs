#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::msgs::virtual_balance::{GetStateResponse, InstantiateMsg, QueryMsg};
use mock::mock::MockApp;

pub struct Mockvirtual_balance(Addr);
impl Mockvirtual_balance {
    pub fn addr(&self) -> &Addr {
        &self.0
    }
}

impl Mockvirtual_balance {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        router: Addr,
        admin: Option<Addr>,
    ) -> Self {
        let msg = mock_virtual_balance_instantiate_msg(router, admin);
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid virtual_balance", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_virtual_balance_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }

    pub fn query_state(&self, app: &MockApp) -> GetStateResponse {
        app.wrap()
            .query_wasm_smart::<GetStateResponse>(
                self.addr().clone().into_string(),
                &mock_query_get_state(),
            )
            .unwrap()
    }
}

pub fn mock_virtual_balance() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query);
    Box::new(contract)
}

pub fn mock_virtual_balance_instantiate_msg(router: Addr, admin: Option<Addr>) -> InstantiateMsg {
    InstantiateMsg { router, admin }
}

pub fn mock_query_get_state() -> QueryMsg {
    QueryMsg::GetState {}
}
