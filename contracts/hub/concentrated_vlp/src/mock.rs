#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::{
    fee::Fee,
    msgs::vlp::concentrated::msg::{ExecuteMsg, GetStateResponse, InstantiateMsg, QueryMsg},
    token::Pair,
};

use crate::{
    contract::{execute, instantiate, query, reply},
    migrate::migrate,
};
use mock::mock::MockApp;

pub struct MockConcentratedVlp(Addr);

impl MockConcentratedVlp {
    pub fn addr(&self) -> &Addr {
        &self.0
    }

    #[allow(clippy::too_many_arguments)]
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        virtual_balance: Addr,
        pair: Pair,
        fee: Fee,
        execute: Option<ExecuteMsg>,
        admin: Addr,
    ) -> Self {
        let msg = mock_concentrated_vlp_instantiate_msg(
            virtual_balance,
            pair,
            fee,
            execute,
            admin,
        );
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Concentrated VLP", None);

        Self(res.unwrap())
    }

    pub fn query_state(&self, app: &MockApp) -> GetStateResponse {
        app.wrap()
            .query_wasm_smart::<GetStateResponse>(
                self.addr().clone().into_string(),
                &mock_query_get_state(),
            )
            .unwrap()
    }
}

pub fn mock_concentrated_vlp() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query)
        .with_reply(reply)
        .with_migrate(migrate);
    Box::new(contract)
}

pub fn mock_concentrated_vlp_instantiate_msg(
    virtual_balance: Addr,
    pair: Pair,
    fee: Fee,
    execute: Option<ExecuteMsg>,
    admin: Addr,
) -> InstantiateMsg {
    InstantiateMsg {
        virtual_balance_contract: virtual_balance,
        pair,
        fee,
        execute,
        admin,
        fee_tier_bps: 500,
        tick_spacing: 10,
    }
}

pub fn mock_query_get_state() -> QueryMsg {
    QueryMsg::State {}
}
