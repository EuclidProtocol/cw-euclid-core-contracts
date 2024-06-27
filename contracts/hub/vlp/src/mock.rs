#![cfg(all(not(target_arch = "wasm32")))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Coin, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::fee::Fee;
use euclid::msgs::vlp::{ExecuteMsg, InstantiateMsg, QueryMsg};
use euclid::testing::{ExecuteResult, MockApp};
use euclid::token::Pair;

pub struct MockVlp(Addr);

impl MockVlp {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        router: String,
        vcoin: String,
        pair: Pair,
        fee: Fee,
        execute: Option<ExecuteMsg>,
    ) -> Self {
        let msg = mock_vlp_instantiate_msg(router, vcoin, pair, fee, execute);
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid vlp", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_vlp_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }
}

pub fn mock_vlp() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_vlp_instantiate_msg(
    router: String,
    vcoin: String,
    pair: Pair,
    fee: Fee,
    execute: Option<ExecuteMsg>,
) -> InstantiateMsg {
    InstantiateMsg {
        router,
        vcoin,
        pair,
        fee,
        execute,
    }
}

// pub fn mock_vlp_send_msg() -> ExecuteMsg {

// }
