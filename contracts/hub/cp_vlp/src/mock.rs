#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::admin::EuclidAdmin;
use euclid::fee::Fee;
use euclid::msgs::vlp::cp::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg};
use euclid::token::Pair;
use mock::mock::MockApp;

pub struct MockCpVlp(Addr);

impl MockCpVlp {
    pub fn addr(&self) -> &Addr {
        &self.0
    }
    #[allow(clippy::too_many_arguments)]
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        router: Addr,
        virtual_balance: Addr,
        pair: Pair,
        fee: Fee,
        execute: Option<ExecuteMsg>,
        admin: EuclidAdmin,
    ) -> Self {
        let msg = mock_cp_vlp_instantiate_msg(router, virtual_balance, pair, fee, execute, admin);
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid vlp", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_vlp_send_msg();

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

    pub fn query_admin(&self, app: &MockApp) -> EuclidAdmin {
        app.wrap()
            .query_wasm_smart::<EuclidAdmin>(
                self.addr().clone().into_string(),
                &QueryMsg::GetAdmin {},
            )
            .unwrap()
    }
}

pub fn mock_cp_vlp() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_cp_vlp_instantiate_msg(
    router: Addr,
    virtual_balance: Addr,
    pair: Pair,
    fee: Fee,
    execute: Option<ExecuteMsg>,
    admin: EuclidAdmin,
) -> InstantiateMsg {
    InstantiateMsg {
        router,
        virtual_balance_contract: virtual_balance,
        pair,
        fee,
        execute,
        admin,
    }
}

// pub fn mock_vlp_send_msg() -> ExecuteMsg {

// }

pub fn mock_query_get_state() -> QueryMsg {
    QueryMsg::State {}
}
