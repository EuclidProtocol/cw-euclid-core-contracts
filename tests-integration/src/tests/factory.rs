#![cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;

use cosmwasm_std::{coin, Addr, Coin, Uint128};
use cw20::Cw20Contract;
use cw_orch::prelude::{
    ContractInstance, CwOrchExecute, CwOrchInstantiate, CwOrchQuery, CwOrchUpload,
};
use cw_orch_interchain::{prelude::*, InterchainEnv};
use escrow::{mock::mock_escrow, EscrowContract};
use euclid::{
    chain::ChainUid,
    fee::DenomFees,
    msgs::{
        escrow::StateResponse as EscrowStateResponse,
        factory::{AllPoolsResponse, ExecuteMsgFns, StateResponse},
        router::{RegisterFactoryChainIbc, VlpResponse},
        virtual_balance::GetStateResponse,
        vlp::GetLiquidityResponse,
    },
    token::{Pair, PairWithDenomAndAmount, Token, TokenWithDenom, TokenWithDenomAndAmount},
};
use factory::{
    mock::{mock_factory, MockFactory},
    FactoryContract,
};
use mock::{mock::mock_app, mock_builder::MockEuclidBuilder};
use router::RouterContract;
use virtual_balance::VirtualBalanceContract;
use vlp::VlpContract;

#[test]
fn test_proper_instantiation() {
    let mut factory = mock_app(None);
    let andr = MockEuclidBuilder::new(&mut factory, "admin")
        .with_wallets(vec![
            ("owner", vec![coin(1000, "eucl")]),
            ("recipient1", vec![]),
            ("recipient2", vec![]),
        ])
        .with_contracts(vec![("escrow", mock_escrow()), ("factory", mock_factory())])
        .build(&mut factory);
    let owner = andr.get_wallet("owner");

    let escrow_code_id = 1;
    let factory_code_id = 2;
    let cw20_code_id = 3;
    let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
    let router_contract = "router_contract".to_string();

    let mock_factory = MockFactory::instantiate(
        &mut factory,
        factory_code_id,
        owner.clone(),
        router_contract.clone(),
        chain_uid.clone(),
        escrow_code_id,
        cw20_code_id,
        true,
    );

    let state_response = MockFactory::query_state(&mock_factory, &mut factory);
    let expected_state_id = StateResponse {
        chain_uid,
        router_contract,
        hub_channel: None,
        admin: owner.clone().into_string(),
        is_native: true,
        cw20_code_id,
        escrow_code_id,
        partner_fees_collected: DenomFees {
            totals: HashMap::new(),
        },
    };
    assert_eq!(state_response, expected_state_id);
}

#[test]
fn test_create_pool_with_funds() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("osmosis", &sender), ("nibiru", &sender)]);
    let osmosis = interchain.get_chain("osmosis").unwrap();
    let nibiru = interchain.get_chain("nibiru").unwrap();

    osmosis
        .set_balance(
            sender.clone(),
            vec![
                Coin::new(100000000000000, "osmo"),
                Coin::new(100000000000000, "eucl"),
            ],
        )
        .unwrap();

    let factory_osmosis = FactoryContract::new(osmosis.clone());
    let escrow_osmosis = EscrowContract::new(osmosis.clone());
    let cw20_osmosis = Cw20Contract::new(osmosis.clone());
    let router_nibiru = RouterContract::new(nibiru.clone());
    let virtual_balance_nibiru = VirtualBalanceContract::new(nibiru.clone());
    let vlp_nibiru = VlpContract::new(nibiru.clone());

    factory_osmosis.upload().unwrap();
    escrow_osmosis.upload().unwrap();
    cw20_osmosis.upload().unwrap();
    router_nibiru.upload().unwrap();
    virtual_balance_nibiru.upload().unwrap();
    vlp_nibiru.upload().unwrap();

    router_nibiru
        .instantiate(
            &euclid::msgs::router::InstantiateMsg {
                vlp_code_id: 3,
                virtual_balance_code_id: 2,
            },
            None,
            None,
        )
        .unwrap();

    factory_osmosis
        .instantiate(
            &euclid::msgs::factory::InstantiateMsg {
                router_contract: router_nibiru.address().unwrap().into_string(),
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                escrow_code_id: 2,
                cw20_code_id: 3,
                is_native: false,
            },
            None,
            None,
        )
        .unwrap();

    // Set up channel from osmosis to nibiru
    let channel_receipt = interchain
        .create_contract_channel(&factory_osmosis, &router_nibiru, "counter-1", None)
        .unwrap();

    // After channel creation is complete, we get the channel id, which is necessary for ICA remote execution
    let osmosis_channel = channel_receipt
        .interchain_channel
        .get_chain("osmosis")
        .unwrap()
        .channel
        .unwrap();

    // Update Hub Channel
    factory_osmosis
        .update_hub_channel(osmosis_channel.to_string())
        .unwrap();

    let register_factory_request = router_nibiru
        .execute(
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_uid: ChainUid::create("osmosis".to_string()).unwrap(),
                chain_info: euclid::msgs::router::RegisterFactoryChainType::Ibc(
                    RegisterFactoryChainIbc {
                        channel: osmosis_channel.to_string(),
                        timeout: None,
                    },
                ),
            },
            None,
        )
        .unwrap();

    let _ = interchain
        .await_packets("nibiru", register_factory_request)
        .unwrap();

    // // Register escrow
    let register_escrow_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterEscrow {
                token: TokenWithDenom {
                    token: Token::create("osmo".to_string()).unwrap(),
                    token_type: euclid::token::TokenType::Native {
                        denom: "osmo".to_string(),
                    },
                },
                timeout: None,
            },
            None,
        )
        .unwrap();

    let _ = interchain
        .await_packets("osmosis", register_escrow_request)
        .unwrap();

    // Need to request register escrow first
    let create_pool_with_funds_request = factory_osmosis
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestPoolCreation {
                pair: PairWithDenomAndAmount {
                    token_1: TokenWithDenomAndAmount {
                        token: Token::create("eucl".to_string()).unwrap(),
                        amount: Uint128::from(10_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "eucl".to_string(),
                        },
                    },
                    token_2: TokenWithDenomAndAmount {
                        token: Token::create("osmo".to_string()).unwrap(),
                        amount: Uint128::from(100_000u128),
                        token_type: euclid::token::TokenType::Native {
                            denom: "osmo".to_string(),
                        },
                    },
                },
                slippage_tolerance_bps: Some(100),
                timeout: None,
                lp_token_name: "osmosis".to_string(),
                lp_token_symbol: "osmo".to_string(),
                lp_token_decimal: 6,
                lp_token_marketing: None,
            },
            Some(&[coin(100_000u128, "osmo"), coin(10_000u128, "eucl")]),
        )
        .unwrap();

    let _ = interchain
        .await_packets("osmosis", create_pool_with_funds_request)
        .unwrap();

    let all_pools_query: AllPoolsResponse = factory_osmosis
        .query(&euclid::msgs::factory::QueryMsg::GetAllPools {})
        .unwrap();
    println!("all pools query: {:?}", all_pools_query);

    let vlp_query: VlpResponse = router_nibiru
        .query(&euclid::msgs::router::QueryMsg::GetVlp {
            pair: Pair::new(
                Token::create("osmo".to_string()).unwrap(),
                Token::create("eucl".to_string()).unwrap(),
            )
            .unwrap(),
        })
        .unwrap();
    println!("vlp query: {:?}", vlp_query);

    // Got this address from the query above
    vlp_nibiru.set_address(&Addr::unchecked("contract2"));

    let liquidity_query: GetLiquidityResponse = vlp_nibiru
        .query(&euclid::msgs::vlp::QueryMsg::Liquidity {})
        .unwrap();
    println!("liquidity query: {:?}", liquidity_query);

    virtual_balance_nibiru.set_address(&Addr::unchecked("contract1"));

    let vbalance_query: GetStateResponse = virtual_balance_nibiru
        .query(&euclid::msgs::virtual_balance::QueryMsg::GetState {})
        .unwrap();
    println!("vbalance state is: {:?}", vbalance_query);

    escrow_osmosis.set_address(&Addr::unchecked("contract1"));
    let escrow_query: EscrowStateResponse = escrow_osmosis
        .query(&euclid::msgs::escrow::QueryMsg::State {})
        .unwrap();
    println!("escrow state is: {:?}", escrow_query);
}
