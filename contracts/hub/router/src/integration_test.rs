#[cfg(test)]
mod tests {

    const USER: &str = "user";
    const NATIVE_DENOM: &str = "native";
    const IBC_DENOM_1: &str = "ibc/denom1";
    const IBC_DENOM_2: &str = "ibc/denom2";
    const SUPPLY: u128 = 1_000_000;
    use cosmwasm_std::{coin, from_json, Addr, Coin, CosmosMsg, Empty, SubMsg, Uint128, WasmMsg};
    use cw_multi_test::{App, AppBuilder, BankSudo, Contract, ContractWrapper, Executor};
    use euclid::msgs::router::{InstantiateMsg, QueryMsg, StateResponse};
    use euclid::testing::{mock_app, MockEuclidBuilder};

    use crate::contract::{execute, instantiate, query, reply};
    use crate::mock::{mock_router, MockRouter};

    // // Mock application setup
    // fn mock_app() -> App {
    //     AppBuilder::new().build(|_router, _api, _storage| {})
    // }

    // Define the router contract
    fn router_contract() -> Box<dyn Contract<Empty>> {
        Box::new(
            ContractWrapper::new_with_empty(execute, instantiate, query).with_reply_empty(reply),
        )
    }

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
                ("vcoin", mock_router()),
            ])
            .build(&mut router);
        let owner = andr.get_wallet("owner");
        let recipient_1 = andr.get_wallet("recipient1");
        let recipient_2 = andr.get_wallet("recipient2");

        // let router_code_id = andr.get_code_id(&mut router, "router");
        let router_code_id = 3;

        let router: MockRouter = MockRouter::instantiate(
            &mut router,
            router_code_id,
            owner.clone(),
            Some(owner.clone().into_string()),
            1,
            2,
        );

        //

        // let mut app = mock_app();
        // let owner = Addr::unchecked("owner");

        // // Store the router contract code
        // let router_code_id = app.store_code(router_contract());

        // // Instantiate the router contract
        // let instantiate_msg = InstantiateMsg {
        //     vlp_code_id: 1,
        //     vcoin_code_id: 2,
        // };

        // let contract_addr = app
        //     .instantiate_contract(
        //         router_code_id,
        //         owner.clone(),
        //         &instantiate_msg,
        //         &[],
        //         "Router Contract",
        //         None,
        //     )
        //     .unwrap();

        // // Query the state to verify the instantiation
        // let state: StateResponse = app
        //     .wrap()
        //     .query_wasm_smart(contract_addr.clone(), &QueryMsg::GetState {})
        //     .unwrap();

        // assert_eq!(state.admin, owner.to_string());
        // assert_eq!(state.vlp_code_id, 1);
        // assert!(state.vcoin_address.is_none());
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
}
