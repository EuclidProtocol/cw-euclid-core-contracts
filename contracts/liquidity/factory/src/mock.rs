#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};

use euclid::{
    chain::ChainUid,
    msgs::factory::{GetEscrowResponse, InstantiateMsg, QueryMsg, StateResponse},
};
use mock::mock::MockApp;

pub struct MockFactory(Addr);
impl MockFactory {
    pub fn addr(&self) -> &Addr {
        &self.0
    }
}

impl MockFactory {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        router_contract: String,
        chain_uid: ChainUid,
        escrow_code_id: u64,
        cw20_code_id: u64,
        is_native: bool,
    ) -> Self {
        let msg = mock_factory_instantiate_msg(
            router_contract,
            chain_uid,
            escrow_code_id,
            cw20_code_id,
            is_native,
        );
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid factory", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_factory_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }

    pub fn query_token_id(&self, app: &MockApp, token_id: String) -> GetEscrowResponse {
        app.wrap()
            .query_wasm_smart::<GetEscrowResponse>(
                self.addr().clone().into_string(),
                &mock_query_get_escrow(token_id),
            )
            .unwrap()
    }

    pub fn query_state(&self, app: &MockApp) -> StateResponse {
        app.wrap()
            .query_wasm_smart::<StateResponse>(
                self.addr().clone().into_string(),
                &mock_query_get_state(),
            )
            .unwrap()
    }
}

pub fn mock_factory() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_factory_instantiate_msg(
    router_contract: String,
    chain_uid: ChainUid,
    escrow_code_id: u64,
    cw20_code_id: u64,
    is_native: bool,
) -> InstantiateMsg {
    InstantiateMsg {
        router_contract,
        chain_uid,
        escrow_code_id,
        cw20_code_id,
        is_native,
    }
}

// pub fn mock_factory_send_msg() -> ExecuteMsg {

// }

pub fn mock_query_get_escrow(token_id: String) -> QueryMsg {
    QueryMsg::GetEscrow { token_id }
}

pub fn mock_query_get_state() -> QueryMsg {
    QueryMsg::GetState {}
}
