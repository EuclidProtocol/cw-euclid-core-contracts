#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Empty};
use cw_multi_test::{Contract, ContractWrapper, Executor};

use euclid::{
    msgs::escrow::{InstantiateMsg, QueryMsg, TokenIdResponse},
    token::{Token, TokenType},
};
use mock::mock::MockApp;

pub struct MockEscrow(Addr);
impl MockEscrow {
    fn addr(&self) -> &Addr {
        &self.0
    }
}

impl MockEscrow {
    pub fn instantiate(
        app: &mut MockApp,
        code_id: u64,
        sender: Addr,
        token_id: Token,
        allowed_denom: Option<TokenType>,
    ) -> Self {
        let msg = mock_escrow_instantiate_msg(token_id, allowed_denom);
        let res = app.instantiate_contract(code_id, sender, &msg, &[], "Euclid escrow", None);

        Self(res.unwrap())
    }

    // pub fn execute_send(&self, app: &mut MockApp, sender: Addr, funds: &[Coin]) -> ExecuteResult {
    //     let msg = mock_escrow_send_msg();

    //     self.execute(app, &msg, sender, funds)
    // }

    pub fn query_token_id(&self, app: &MockApp) -> TokenIdResponse {
        app.wrap()
            .query_wasm_smart::<TokenIdResponse>(
                self.addr().clone().into_string(),
                &mock_query_token_id(),
            )
            .unwrap()
    }
}

pub fn mock_escrow() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_escrow_instantiate_msg(
    token_id: Token,
    allowed_denom: Option<TokenType>,
) -> InstantiateMsg {
    InstantiateMsg {
        token_id,
        allowed_denom,
    }
}

// pub fn mock_escrow_send_msg() -> ExecuteMsg {

// }

pub fn mock_query_token_id() -> QueryMsg {
    QueryMsg::TokenId {}
}
