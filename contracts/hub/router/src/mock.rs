#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};

use euclid::msgs::router::{InstantiateMsg, QueryMsg, StateResponse};
use mock::mock::MockApp;

pub struct MockRouter(Addr);
impl MockRouter {
    pub fn addr(&self) -> &Addr {
        &self.0
    }
}

impl MockRouter {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        vlp_code_id: u64,
        virtual_balance_code_id: u64,
    ) -> Self {
        let msg = mock_router_instantiate_msg(vlp_code_id, virtual_balance_code_id);
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid router", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_router_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }

    // pub fn query_state(&self, app: &MockApp, token_id: impl Into<String>) -> Addr
    // {
    //     app.wrap().query(request)
    //     Addr::unchecked(self.query::<StateResponse>(app, mock_query_state()).owner)
    // }

    pub fn query_state(&self, app: &MockApp) -> StateResponse {
        app.wrap()
            .query_wasm_smart::<StateResponse>(
                self.addr().clone().into_string(),
                &mock_query_state(),
            )
            .unwrap()
    }
}

pub fn mock_router() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_router_instantiate_msg(vlp_code_id: u64, virtual_balance_code_id: u64) -> InstantiateMsg {
    InstantiateMsg {
        vlp_code_id,
        virtual_balance_code_id,
    }
}

// pub fn mock_router_send_msg() -> ExecuteMsg {

// }

pub fn mock_query_state() -> QueryMsg {
    QueryMsg::GetState {}
}
