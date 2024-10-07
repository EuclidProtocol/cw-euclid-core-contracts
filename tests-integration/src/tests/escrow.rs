#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::coins;
use cosmwasm_std::Addr;
use cosmwasm_std::CosmosMsg;
use cosmwasm_std::Uint128;
use cw_orch::prelude::*;
use cw_orch_interchain::types::IbcPacketOutcome;
use escrow::EscrowContract;
use euclid::chain::ChainUid;
use euclid::msgs::escrow::AllowedDenomsResponse;
use euclid::msgs::escrow::QueryMsgFns;
use euclid::msgs::escrow::{ExecuteMsgFns, InstantiateMsg};
use euclid::msgs::factory::InstantiateMsg as FactoryInstantiateMsg;
use euclid::msgs::router::InstantiateMsg as RouterInstantiateMsg;
use euclid::token::TokenWithDenom;
use euclid::token::{Token, TokenType};
use factory::FactoryContract;

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;
use cw_orch_interchain::prelude::*;
use cw_orch_interchain::InterchainEnv;
use router::RouterContract;

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
            None,
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
fn test_factory_ibc() {
    // Here `juno-1` is the chain-id and `juno` is the address prefix for this chain
    let sender = Addr::unchecked("sender_for_all_chains").into_string();

    let interchain = MockInterchainEnv::new(vec![("juno", &sender), ("osmosis", &sender)]);

    let juno = interchain.chain("juno").unwrap();
    let osmosis = interchain.chain("osmosis").unwrap();

    juno.set_balance(sender.clone(), vec![Coin::new(100000000000000, "juno")])
        .unwrap();

    let factory_juno = FactoryContract::new(juno.clone());
    let escrow_juno = EscrowContract::new(juno);
    let router_osmo = RouterContract::new(osmosis);
    //Upload contracts
    escrow_juno.upload().unwrap();
    factory_juno.upload().unwrap();
    router_osmo.upload().unwrap();

    // escrow_juno
    //     .instantiate(
    //         &InstantiateMsg {
    //             token_id: Token::create("token".to_string()).unwrap(),
    //             allowed_denom: Some(TokenType::Native {
    //                 denom: "juno".to_string(),
    //             }),
    //         },
    //         None,
    //         None,
    //     )
    //     .unwrap();
    factory_juno
        .instantiate(
            &FactoryInstantiateMsg {
                router_contract: "factory-juno".to_string(),
                chain_uid: ChainUid::create("uid".to_string()).unwrap(),
                escrow_code_id: 1,
                cw20_code_id: 2,
                is_native: false,
            },
            None,
            None,
        )
        .unwrap();
    router_osmo
        .instantiate(
            &RouterInstantiateMsg {
                vlp_code_id: 1,
                virtual_balance_code_id: 2,
            },
            None,
            None,
        )
        .unwrap();

    let channel_receipt = interchain
        .create_contract_channel(&factory_juno, &router_osmo, "counter-1", None)
        .unwrap();

    // After channel creation is complete, we get the channel id, which is necessary for ICA remote execution
    let juno_channel = channel_receipt
        .interchain_channel
        .get_chain("juno")
        .unwrap()
        .channel
        .unwrap();

    // Add a hub channel to factory-juno
    factory_juno
        .execute(
            &euclid::msgs::factory::ExecuteMsg::UpdateHubChannel {
                new_channel: juno_channel.to_string(),
            },
            None,
        )
        .unwrap();

    let token_with_denom = TokenWithDenom {
        token: Token::create("juno".to_string()).unwrap(),
        token_type: TokenType::Native {
            denom: "juno".to_string(),
        },
    };

    // we should request to register escrow on factory-juno
    let tx_response = factory_juno
        .execute(
            &euclid::msgs::factory::ExecuteMsg::RequestRegisterEscrow {
                token: token_with_denom.clone(),
                timeout: None,
            },
            None,
        )
        .unwrap();

    // This broadcasts a transaction on the client
    // It sends an IBC packet to the host
    let amount = Uint128::from(100u128);

    let packet_lifetime = interchain.wait_ibc("juno", tx_response).unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { ack, .. } = &packet_lifetime.packets[0].outcome {
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };

    // let tx_response = factory_juno
    //     .execute(
    //         &euclid::msgs::factory::ExecuteMsg::DepositToken {
    //             asset_in: token_with_denom,
    //             amount_in: amount,
    //             recipient: None,
    //             timeout: None,
    //         },
    //         Some(&coins(100u128, "juno")),
    //     )
    //     .unwrap();
}
