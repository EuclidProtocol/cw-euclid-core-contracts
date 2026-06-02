use cosmwasm_std::{
    testing::{message_info, mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage},
    Addr, OwnedDeps, Response,
};
use euclid::msgs::pool_factory::InstantiateMsg;

use crate::contract::instantiate;

pub type MockDeps = OwnedDeps<MockStorage, MockApi, MockQuerier>;

pub const TEST_MAIN_FACTORY: &str = "cosmwasm1main_factory_addr";

pub fn init(deps: &mut MockDeps) -> Response {
    let api = deps.api;
    let main_factory = api.addr_make("main_factory");
    let sender = api.addr_make("sender");
    let info = message_info(&sender, &[]);
    let msg = InstantiateMsg {
        main_factory_address: main_factory.to_string(),
    };
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

pub fn init_with_main_factory(deps: &mut MockDeps, main_factory: &Addr) -> Response {
    let api = deps.api;
    let sender = api.addr_make("sender");
    let info = message_info(&sender, &[]);
    let msg = InstantiateMsg {
        main_factory_address: main_factory.to_string(),
    };
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap()
}

/// Build a fresh MockDeps and run the standard instantiate.
pub fn setup() -> (MockDeps, Addr) {
    let mut deps = mock_dependencies();
    let main_factory = deps.api.addr_make("main_factory");
    init_with_main_factory(&mut deps, &main_factory);
    (deps, main_factory)
}
