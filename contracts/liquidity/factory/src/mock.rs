#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Empty, Uint128};
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
        lp_code_id: u64,
        position_token_code_id: u64,
        is_native: bool,
        relayer_contract: Addr,
        rate_limit_fee_recipient: Addr,
        rate_limit_fee_denom: String,
        rate_limit_free_limit: Uint128,
    ) -> Self {
        let msg = mock_factory_instantiate_msg(
            router_contract,
            chain_uid,
            escrow_code_id,
            lp_code_id,
            position_token_code_id,
            is_native,
            relayer_contract,
            rate_limit_fee_recipient,
            rate_limit_fee_denom,
            rate_limit_free_limit,
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
    lp_code_id: u64,
    position_token_code_id: u64,
    is_native: bool,
    relayer_contract: Addr,
    rate_limit_fee_recipient: Addr,
    rate_limit_fee_denom: String,
    rate_limit_free_limit: Uint128,
) -> InstantiateMsg {
    InstantiateMsg {
        router_contract,
        chain_uid,
        escrow_code_id,
        lp_code_id,
        position_token_code_id,
        is_native,
        relayer_contract,
        rate_limit_fee_recipient,
        rate_limit_fee_denom,
        rate_limit_free_limit,
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
