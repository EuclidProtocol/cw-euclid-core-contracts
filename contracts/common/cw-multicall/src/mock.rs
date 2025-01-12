#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};

use euclid_utils::msgs::multicall::{InstantiateMsg, MultiQuery, MultiQueryResponse, QueryMsg};
use mock::mock::MockApp;

pub struct MockEscrow(Addr);
impl MockEscrow {
    fn addr(&self) -> &Addr {
        &self.0
    }
}

impl MockEscrow {
    pub fn instantiate(app: &mut MockApp, code_id: u64, sender: Addr) -> Self {
        let msg = mock_cw_multi_call_msg();
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid escrow", None);

        Self(res.unwrap())
    }

    pub fn query_multi_queries(
        &self,
        app: &MockApp,
        queries: Vec<MultiQuery>,
    ) -> MultiQueryResponse {
        app.wrap()
            .query_wasm_smart::<MultiQueryResponse>(
                self.addr().clone().into_string(),
                &mock_query_multi_query(queries),
            )
            .unwrap()
    }
}

pub fn mock_cw_multi_call() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_cw_multi_call_msg() -> InstantiateMsg {
    InstantiateMsg {}
}

pub fn mock_query_multi_query(queries: Vec<MultiQuery>) -> QueryMsg {
    QueryMsg::MultiQuery { queries }
}
