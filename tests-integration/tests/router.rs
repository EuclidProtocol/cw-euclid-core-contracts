#![cfg(not(target_arch = "wasm32"))]

use cosmwasm_std::{coin, Addr};
use euclid::{mock::mock_app, mock_builder::MockEuclidBuilder, msgs::router::StateResponse};

use router::mock::{mock_router, MockRouter};

use vcoin::mock::mock_vcoin;
use vlp::mock::mock_vlp;

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;

#[test]
fn test_proper_instantiation() {
    let mut router = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut router, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![
            ("router", mock_router()),
            ("vlp", mock_vlp()),
            ("vcoin", mock_vcoin()),
        ])
        .build(&mut router);
    let owner = andr.get_wallet("owner");
    let _recipient_1 = andr.get_wallet("recipient1");
    let _recipient_2 = andr.get_wallet("recipient2");

    let router_code_id = 1;
    let vlp_code_id = 2;
    let vcoin_code_id = 3;

    let mock_router = MockRouter::instantiate(
        &mut router,
        router_code_id,
        owner.clone(),
        vlp_code_id,
        vcoin_code_id,
        Some(owner.clone().into_string()),
    );

    let state = MockRouter::query_state(&mock_router, &mut router);
    let expected_state_response = StateResponse {
        admin: owner.clone().into_string(),
        vlp_code_id,
        vcoin_address: Some(Addr::unchecked(
            "eucl1hrpna9v7vs3stzyd4z3xf00676kf78zpe2u5ksvljswn2vnjp3ys8rp88c",
        )),
    };
    assert_eq!(state, expected_state_response);
}

// #[test]
// fn test_execute_update_vlp_code_id() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     // Register router and vcoin contract codes
//     let router_code = app.store_code(router_contract());

//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: vcoin_code, // Use the registered vcoin_code
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     let update_msg = ExecuteMsg::UpdateVLPCodeId { new_vlp_code_id: 3 };
//     app.execute_contract(owner.clone(), router.clone(), &update_msg, &[]).unwrap();

//     let res: StateResponse = app.wrap().query_wasm_smart(
//         router,
//         &QueryMsg::GetState {},
//     ).unwrap();

//     assert_eq!(res.vlp_code_id, 3);
// }

// #[test]
// fn test_execute_register_factory() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     // Register router and vcoin contract codes
//     let router_code = app.store_code(router_contract());
//     let vcoin_code = app.store_code(vcoin_contract());

//     // Instantiate the router contract with the registered vcoin_code
//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: vcoin_code, // Use the registered vcoin_code
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     let register_msg = ExecuteMsg::RegisterFactory { channel: "channel-1".to_string(), timeout: Some(60) };
//     let res = app.execute_contract(owner.clone(), router.clone(), &register_msg, &[]).unwrap();

//     // Check the events
//     let events = res.events;
//     assert!(events.iter().any(|e| e.ty == "execute" && e.attributes.iter().any(|a| a.key == "method" && a.value == "register_factory")));
//     assert!(events.iter().any(|e| e.ty == "execute" && e.attributes.iter().any(|a| a.key == "channel" && a.value == "channel-1")));
//     assert!(events.iter().any(|e| e.ty == "execute" && e.attributes.iter().any(|a| a.key == "timeout" && a.value == "60")));

//     // Find the IBC packet sent event
//     let ibc_packet_event = events.iter().find(|e| e.ty == "send_packet").expect("IBC packet event not found");

//     // Extract relevant attributes from the event
//     let channel_id = ibc_packet_event.attributes.iter().find(|a| a.key == "packet_src_channel").expect("channel_id not found").value.clone();
//     let data = ibc_packet_event.attributes.iter().find(|a| a.key == "packet_data").expect("data not found").value.clone();

//     assert_eq!(channel_id, "channel-1");

//     // Deserialize the packet data
//     let msg: HubIbcExecuteMsg = from_json(data).unwrap();
//     assert_eq!(msg, HubIbcExecuteMsg::RegisterFactory { router: router.to_string() });
// }

// #[test]
// fn test_query_all_vlps() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     let router_code = app.store_code(router_contract());
//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: 2,
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     // Assume some VLPS are added here

//     let res: AllVlpResponse = app.wrap().query_wasm_smart(
//         router,
//         &QueryMsg::GetAllVlps {},
//     ).unwrap();

//     // Add assertions based on the assumed state of VLPS
//     assert_eq!(res.vlps.len(), 0);  // Modify this based on the state
// }

// #[test]
// fn test_query_vlp() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     let router_code = app.store_code(router_contract());
//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: 2,
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     // Assume a VLP is added here
//     let token_1 = Token {
//         id: "token1".to_string(),
//     };
//     let token_2 = Token {
//         id: "token2".to_string(),
//     };

//     let res: VlpResponse = app.wrap().query_wasm_smart(
//         router,
//         &QueryMsg::GetVlp { token_1: token_1.clone(), token_2: token_2.clone() },
//     ).unwrap();

//     // Add assertions based on the assumed state of VLP
//     assert_eq!(res.token_1, token_1);
//     assert_eq!(res.token_2, token_2);
// }

// #[test]
// fn test_query_all_chains() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     let router_code = app.store_code(router_contract());
//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: 2,
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     // Assume some Chains are added here

//     let res: AllChainResponse = app.wrap().query_wasm_smart(
//         router,
//         &QueryMsg::GetAllChains {},
//     ).unwrap();

//     // Add assertions based on the assumed state of Chains
// #[test]
// fn test_query_all_chains() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     let router_code = app.store_code(router_contract());
//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: 2,
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     // Assume some Chains are added here

//     let res: AllChainResponse = app.wrap().query_wasm_smart(
//         router,
//         &QueryMsg::GetAllChains {},
//     ).unwrap();

//     // Add assertions based on the assumed state of Chains
//     assert_eq!(res.chains.len(), 0);  // Modify this based on the state
// }
//     assert_eq!(res.chains.len(), 0);  // Modify this based on the state
// }

// #[test]
// fn test_query_chain() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     let router_code = app.store_code(router_contract());
//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: 2,
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     // Assume a Chain is added here

//     let chain_id = "chain-id".to_string();

//     let res: ChainResponse = app.wrap().query_wasm_smart(
//         router,
//         &QueryMsg::GetChain { chain_id: chain_id.clone() },
//     ).unwrap();

//     // Add assertions based on the assumed state of Chain
//     assert_eq!(res.chain.factory_chain_id, chain_id);
// }

// #[test]
// fn test_query_simulate_swap() {
//     let mut app = mock_app();
//     let owner = Addr::unchecked("owner");

//     let router_code = app.store_code(router_contract());
//     let router = app.instantiate_contract(
//         router_code,
//         owner.clone(),
//         &InstantiateMsg {
//             vlp_code_id: 1,
//             vcoin_code_id: 2,
//         },
//         &[],
//         "router",
//         None,
//     ).unwrap();

//     // Assume some swaps and state here

//     let swap_msg = QuerySimulateSwap {
//         factory_chain: "factory_chain".to_string(),
//         to_address: "to_address".to_string(),
//         to_chain_id: "to_chain_id".to_string(),
//         asset_in: Token {
//             id: "asset_in".to_string(),
//         },
//         amount_in: Uint128::from(100u128),
//         min_amount_out: Uint128::from(50u128),
//         swaps: vec![NextSwap {
//             vlp_address: "vlp_address".to_string(),

//         }],
//     };

//     let res: SimulateSwapResponse = app.wrap().query_wasm_smart(
//         router,
//         &QueryMsg::SimulateSwap(swap_msg),
//     ).unwrap();

//     // Add assertions based on the expected response
//     assert_eq!(res.amount_out, Uint128::from(50u128));  // Modify this based on the state
// }
