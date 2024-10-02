#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::Addr;
use cosmwasm_std::CosmosMsg;
use cw_orch::prelude::*;
use escrow::EscrowContract;
use euclid::chain::ChainUid;
use euclid::msgs::escrow::AllowedDenomsResponse;
use euclid::msgs::escrow::QueryMsgFns;
use euclid::msgs::escrow::{ExecuteMsgFns, InstantiateMsg};
use euclid::msgs::factory::InstantiateMsg as FactoryInstantiateMsg;
use euclid::token::{Token, TokenType};
use factory::FactoryContract;

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;
use cw_orch_interchain::prelude::*;
use cw_orch_interchain::InterchainEnv;

#[test]
fn test_escrow() {
    // Here `juno-1` is the chain-id and `juno` is the address prefix for this chain
    let mut interchain =
        MockBech32InterchainEnv::new(vec![("juno-1", "juno"), ("osmosis-1", "osmo")]);
    let local_juno = interchain.chain("juno-1").unwrap();
    let local_osmo = interchain.chain("osmosis-1").unwrap();
    let test_migaloo = MockBech32::new_with_chain_id("migaloo-1", "migaloo");
    interchain.add_mocks(vec![test_migaloo]);

    let sender = Addr::unchecked("juno16g2rahf5846rxzp3fwlswy08fz8ccuwk03k57y");

    let mock = Mock::new(&sender);
    let contract_counter = EscrowContract::new(mock);

    let upload_res = contract_counter.upload();
    upload_res.unwrap();

    let _res = contract_counter
        .instantiate(
            &InstantiateMsg {
                token_id: Token::create("token".to_string()).unwrap(),
                allowed_denom: None,
            },
            None,
            None,
        )
        .unwrap();

    let native_denom = TokenType::Native {
        denom: "native".to_string(),
    };

    contract_counter
        .add_allowed_denom(native_denom.clone())
        .unwrap();

    let allowed_denoms: AllowedDenomsResponse = contract_counter.allowed_denoms().unwrap();
    assert_eq!(allowed_denoms.denoms, vec![native_denom]);
}

#[test]
fn test_escrow_ibc() {
    let sender = Addr::unchecked("juno16g2rahf5846rxzp3fwlswy08fz8ccuwk03k57y");

    let mock = Mock::new(&sender);
    let contract_counter = EscrowContract::new(mock);

    let upload_res = contract_counter.upload();
    upload_res.unwrap();

    let _res = contract_counter
        .instantiate(
            &InstantiateMsg {
                token_id: Token::create("token".to_string()).unwrap(),
                allowed_denom: None,
            },
            None,
            None,
        )
        .unwrap();

    let native_denom = TokenType::Native {
        denom: "native".to_string(),
    };

    contract_counter
        .add_allowed_denom(native_denom.clone())
        .unwrap();

    let allowed_denoms: AllowedDenomsResponse = contract_counter.allowed_denoms().unwrap();
    assert_eq!(allowed_denoms.denoms, vec![native_denom]);

    // Here `juno-1` is the chain-id and `juno` is the address prefix for this chain
    let sender = Addr::unchecked("sender_for_all_chains").into_string();
    let interchain = MockInterchainEnv::new(vec![("juno", &sender), ("osmosis", &sender)]);

    let juno = interchain.chain("juno").unwrap();
    let osmosis = interchain.chain("osmosis").unwrap();

    let client = FactoryContract::new(juno);
    let host = FactoryContract::new(osmosis);

    client.upload().unwrap();
    host.upload().unwrap();
    client
        .instantiate(
            &FactoryInstantiateMsg {
                router_contract: "router-juno".to_string(),
                chain_uid: ChainUid::create("uid".to_string()).unwrap(),
                escrow_code_id: 1,
                cw20_code_id: 2,
                is_native: false,
            },
            None,
            None,
        )
        .unwrap();
    host.instantiate(
        &FactoryInstantiateMsg {
            router_contract: "router-osmo".to_string(),
            chain_uid: ChainUid::create("uid".to_string()).unwrap(),
            escrow_code_id: 1,
            cw20_code_id: 2,
            is_native: false,
        },
        None,
        None,
    )
    .unwrap();

    let channel_receipt = interchain
        .create_contract_channel(&client, &host, "counter-1", None)
        .unwrap();

    // After channel creation is complete, we get the channel id, which is necessary for ICA remote execution
    let juno_channel = channel_receipt
        .interchain_channel
        .get_chain("juno")
        .unwrap()
        .channel
        .unwrap();

    /// This broadcasts a transaction on the client
    /// It sends an IBC packet to the host
    let tx_response = client.send_msgs(
        juno_channel.to_string(),
        vec![CosmosMsg::Bank(cosmwasm_std::BankMsg::Burn {
            amount: vec![cosmwasm_std::coin(100u128, "uosmo")],
        })],
        None,
    )?;
}
