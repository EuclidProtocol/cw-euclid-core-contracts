#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::{to_json_binary, Addr, Uint128};
use cw20_std::Cw20Coin;
use cw_orch::prelude::*;
use cw_orch_interchain::{core::InterchainEnv, prelude::*};
use escrow::EscrowContract;
use euclid::{
    chain::ChainUid,
    error::ContractError,
    msgs::{
        escrow::{
            AllowedDenomsResponse, ExecuteMsgFns, InstantiateMsg, QueryMsgFns, StateResponse,
        },
        factory::QueryMsgFns as FactoryQueryMsgFns,
    },
    token::{Pair, Token, TokenType, TokenWithDenom},
};

use crate::helpers::chains::{get_escrow, setup_factory, setup_router};
use crate::helpers::relayer::relay_factory_router_factory;

#[test]
fn test_escrow() {
    // Here `juno-1` is the chain-id and `juno` is the address prefix for this chain
    let mut interchain =
        MockBech32InterchainEnv::new(vec![("juno-1", "juno"), ("osmosis-1", "osmo")]);
    let _local_juno = interchain.get_chain("juno-1").unwrap();
    let _local_osmo = interchain.get_chain("osmosis-1").unwrap();
    let test_migaloo = MockBech32::new_with_chain_id("migaloo-1", "migaloo");
    interchain.add_mocks(vec![test_migaloo]);

    let sender = Addr::unchecked("juno16g2rahf5846rxzp3fwlswy08fz8ccuwk03k57y");

    let mock = Mock::new(sender);
    let escrow_contract = EscrowContract::new(mock);

    let upload_res = escrow_contract.upload();
    upload_res.unwrap();

    let _res = escrow_contract
        .instantiate(
            &InstantiateMsg {
                token_id: Token::create("token".to_string()).unwrap(),
                allowed_denom: None,
            },
            None,
            &[],
        )
        .unwrap();

    let native_denom = TokenType::Native {
        denom: "native".to_string(),
    };

    escrow_contract
        .add_allowed_denom(native_denom.clone())
        .unwrap();

    let allowed_denoms: AllowedDenomsResponse = escrow_contract.allowed_denoms().unwrap();
    assert_eq!(allowed_denoms.denoms, vec![native_denom]);
}

#[test]
fn test_cw20_deposit() {
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let factory_chain_id = "andromeda";
    let router_chain_id = "osmosis";

    let chains = vec![
        (router_chain_id, sender.as_str()),
        (factory_chain_id, sender.as_str()),
    ];
    let interchain = MockInterchainEnv::new(chains.clone());
    let mut factory_chain = interchain.get_chain(factory_chain_id).unwrap();
    let router_chain = interchain.get_chain(router_chain_id).unwrap();

    let token_a_id: String = format!("token.a.{}", router_chain_id);
    let token_b_id: String = format!("token.b.{}", router_chain_id);

    let sender = factory_chain.addr_make("sender_for_all_chains");

    let router_contract = setup_router(&router_chain).unwrap();

    let factory_contract = setup_factory(
        &interchain,
        factory_chain_id,
        router_chain_id,
        &router_contract,
    )
    .unwrap();

    // query factory state
    let factory_state = factory_contract.get_state().unwrap();
    println!("factory state: {:?}", factory_state);

    let cw20_code_id = factory_state.cw20_code_id;

    let cw20_init_response = factory_chain
        .instantiate(
            cw20_code_id,
            &euclid::msgs::cw20::InstantiateMsg {
                name: "Test CW20".to_string(),
                symbol: "TEST".to_string(),
                decimals: 18,
                initial_balances: vec![
                    Cw20Coin {
                        address: sender.to_string(),
                        amount: Uint128::from(100000000000000u128),
                    },
                    Cw20Coin {
                        address: factory_contract.address().unwrap().to_string(),
                        amount: Uint128::from(100000000000000u128),
                    },
                ],
                mint: None,
                marketing: None,
                vlp: "vlp".to_string(),
                factory: factory_contract.address().unwrap(),
                token_pair: Pair::new(
                    Token::create(token_a_id.clone()).unwrap(),
                    Token::create(token_b_id.clone()).unwrap(),
                )
                .unwrap(),
            },
            None,
            Some(&factory_contract.address().unwrap()),
            &[],
        )
        .unwrap();
    // extract cw20 address from instantiate event
    let cw20_address = cw20_init_response
        .events
        .iter()
        .find(|event| event.ty == "instantiate")
        .and_then(|event| {
            event
                .attributes
                .iter()
                .find(|attr| attr.key == "_contract_address")
        })
        .map(|attr| attr.value.clone())
        .expect("No contract address found in instantiate event");

    let cw20_token = Token::create(cw20_address.clone()).unwrap();
    // Register escrow
    let register_escrow_request = factory_contract
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterDenom {
                token: TokenWithDenom {
                    token: cw20_token.clone(),
                    token_type: TokenType::Smart {
                        contract_address: cw20_address.clone(),
                    },
                }
                .clone(),
                timeout: None,
            },
            &[],
        )
        .unwrap();

    let factory_chain_uid = ChainUid::create(factory_chain_id.to_string()).unwrap();
    relay_factory_router_factory(
        register_escrow_request.events,
        &factory_contract,
        &router_contract,
        &factory_chain_uid,
    )
    .unwrap();

    let mut escrow_contract = get_escrow(&factory_contract, cw20_address.as_str());

    // Try adding existing denom
    escrow_contract.set_sender(&factory_contract.address().unwrap());
    let err: ContractError = escrow_contract
        .add_allowed_denom(TokenType::Smart {
            contract_address: cw20_address.clone(),
        })
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err, ContractError::DuplicateDenominations {});

    // set factory as sender
    factory_chain.set_sender(factory_contract.address().unwrap());
    factory_chain
        .execute(
            &euclid::msgs::cw20::ExecuteMsg::Send {
                contract: escrow_contract.address().unwrap().to_string(),
                amount: Uint128::from(1000u128),
                msg: to_json_binary(&euclid::msgs::escrow::cw20::EscrowCw20HookMsg::Deposit {})
                    .unwrap(),
            },
            &[],
            &Addr::unchecked(&cw20_address),
        )
        .unwrap();

    // Query escrow state
    let escrow_state = escrow_contract.state().unwrap();

    let expected_escrow_state = StateResponse {
        token: cw20_token.clone(),
        factory_address: factory_contract.address().unwrap(),
        total_amount: Uint128::from(1000u128),
    };
    assert_eq!(escrow_state, expected_escrow_state);
}
