#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::{
    admin::EuclidAdmin,
    msgs::virtual_balance::msg::{InstantiateMsg, QueryMsg, State},
};
use mock::mock::MockApp;

pub struct MockVirtualBalance(Addr);
impl MockVirtualBalance {
    pub fn addr(&self) -> &Addr {
        &self.0
    }
}

impl MockVirtualBalance {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        router: Addr,
        admin: Option<EuclidAdmin>,
    ) -> Self {
        let msg = mock_virtual_balance_instantiate_msg(router, admin);
        let res =
            app.instantiate_contract(code_id, sender, &msg, &[], "Euclid virtual_balance", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_virtual_balance_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }

    pub fn query_state(&self, app: &MockApp) -> State {
        app.wrap()
            .query_wasm_smart::<State>(self.addr().clone().into_string(), &mock_query_get_state())
            .unwrap()
    }

    pub fn query_admin(&self, app: &MockApp) -> EuclidAdmin {
        app.wrap()
            .query_wasm_smart::<EuclidAdmin>(
                self.addr().clone().into_string(),
                &QueryMsg::GetAdmin {},
            )
            .unwrap()
    }
}

pub fn mock_virtual_balance() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query);
    Box::new(contract)
}

pub fn mock_virtual_balance_instantiate_msg(
    router: Addr,
    admin: Option<EuclidAdmin>,
) -> InstantiateMsg {
    InstantiateMsg { router, admin }
}

pub fn mock_query_get_state() -> QueryMsg {
    QueryMsg::GetState {}
}
