#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query, reply};
use cosmwasm_std::{Addr, Binary, Empty};
use cw_asset::AssetInfo;
use cw_multi_test::{Contract, ContractWrapper, Executor};
use euclid::fee::Fee;
use euclid::msgs::concentrated_vlp::{
    ExecuteMsg, GetStateResponse, InstantiateMsg, PairType, QueryMsg,
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
        router: String,
        virtual_balance: String,
        fee: Fee,
        execute: Option<ExecuteMsg>,
        admin: String,
        pair_type: PairType,
        asset_infos: Vec<AssetInfo>,
        token_code_id: u64,
        factory_addr: String,
        init_params: Option<Binary>,
    ) -> Self {
        let msg = mock_concentrated_vlp_instantiate_msg(
            router,
            virtual_balance,
            fee,
            execute,
            admin,
            pair_type,
            asset_infos,
            token_code_id,
            factory_addr,
            init_params,
        );
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
}

pub fn mock_concentrated_vlp() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new_with_empty(execute, instantiate, query).with_reply(reply);
    Box::new(contract)
}

pub fn mock_concentrated_vlp_instantiate_msg(
    router: String,
    virtual_balance: String,
    fee: Fee,
    execute: Option<ExecuteMsg>,
    admin: String,
    // Concentrated VLP
    pair_type: PairType,
    asset_infos: Vec<AssetInfo>,
    token_code_id: u64,
    factory_addr: String,
    init_params: Option<Binary>,
) -> InstantiateMsg {
    InstantiateMsg {
        router,
        virtual_balance,
        fee,
        execute,
        admin,
        pair_type,
        asset_infos,
        token_code_id,
        factory_addr,
        init_params,
    }
}

pub fn mock_query_get_state() -> QueryMsg {
    QueryMsg::State {}
}
