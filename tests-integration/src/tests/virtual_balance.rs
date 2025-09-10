#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::coin;
use cosmwasm_std::Addr;
use cw_orch::prelude::CwOrchQuery;
use cw_orch_interchain::mock::MockInterchainEnv;
use cw_orch_interchain::prelude::InterchainEnv;
use euclid::error::ContractError;
use euclid::msgs::router::QueryMsgFns;
use euclid::msgs::virtual_balance::ExecuteMsgFns;
use euclid::msgs::virtual_balance::{GetStateResponse, State};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::mock::mock_router;
use router::mock::MockRouter;
use virtual_balance::mock::{mock_virtual_balance, MockVirtualBalance};

use crate::helpers::chains::get_virtual_balance;
use crate::helpers::chains::setup_factory;
use crate::helpers::chains::setup_router;

#[test]
fn test_proper_instantiation() {
    let mut virtual_balance = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut virtual_balance, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![
            ("virtual_balance", mock_virtual_balance()),
            ("router", mock_router()),
        ])
        .build(&mut virtual_balance);
    let owner = andr.get_wallet("owner");

    let virtual_balance_code_id = 1;
    let router_code_id = 2;
    let vlp_code_id = 3;

    let mock_router = MockRouter::instantiate(
        &mut virtual_balance,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        0,
        virtual_balance_code_id,
    );

    let mock_virtual_balance = MockVirtualBalance::instantiate(
        &mut virtual_balance,
        virtual_balance_code_id,
        mock_router.addr().clone(),
        mock_router.addr().clone(),
        None,
    );

    let token_id_response =
        MockVirtualBalance::query_state(&mock_virtual_balance, &virtual_balance);
    let expected_token_id = GetStateResponse {
        state: State {
            router: mock_router.addr().clone().into_string(),
            admin: mock_router.addr().to_owned(),
        },
    };
    assert_eq!(token_id_response, expected_token_id);
}

#[test]
fn update_state_admin() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let factory_chain_id = "andromeda";
    let router_chain_id = "osmosis";

    let chains = vec![
        (router_chain_id, sender.as_str()),
        (factory_chain_id, sender.as_str()),
    ];
    let interchain = MockInterchainEnv::new(chains.clone());
    let factory_chain = interchain.get_chain(factory_chain_id).unwrap();
    let router_chain = interchain.get_chain(router_chain_id).unwrap();

    let sender = factory_chain.addr_make("sender_for_all_chains");
    let other_sender = factory_chain.addr_make("other_sender");

    let router_contract = setup_router(&router_chain).unwrap();

    let virtual_balance_contract = router_contract
        .get_state()
        .unwrap()
        .virtual_balance_address
        .unwrap();

    let virtual_balance_contract = get_virtual_balance(&router_chain, &virtual_balance_contract);

    let state_response: GetStateResponse = virtual_balance_contract
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();
    assert_eq!(state_response.state.admin, sender);

    virtual_balance_contract
        .update_state(Some(other_sender.clone()), None)
        .unwrap();

    // The sender is sending the message but now the other sender is the admin, so this should return an unauthorized error
    let err: ContractError = virtual_balance_contract
        .update_state(Some(sender.clone()), None)
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});
}
