#![cfg(not(target_arch = "wasm32"))]
use cosmwasm_std::Addr;
use cw_orch::prelude::*;
use cw_orch_interchain::types::IbcPacketOutcome;
use escrow::EscrowContract;
use euclid::chain::Chain;
use euclid::chain::ChainUid;
use euclid::chain::IbcChain;
use euclid::msgs::factory::InstantiateMsg as FactoryInstantiateMsg;
use euclid::msgs::router::AllChainResponse;
use euclid::msgs::router::ChainResponse;
use euclid::msgs::router::InstantiateMsg as RouterInstantiateMsg;
use euclid::msgs::router::RegisterFactoryChainIbc;
use factory::FactoryContract;

const _USER: &str = "user";
const _NATIVE_DENOM: &str = "native";
const _IBC_DENOM_1: &str = "ibc/denom1";
const _IBC_DENOM_2: &str = "ibc/denom2";
const _SUPPLY: u128 = 1_000_000;
use cw_orch_interchain::prelude::*;
use cw_orch_interchain::InterchainEnv;
use router::RouterContract;
use virtual_balance::VirtualBalanceContract;

#[test]
fn test_register_factory() {
    // Here `juno-1` is the chain-id and `juno` is the address prefix for this chain
    let sender = Addr::unchecked("sender_for_all_chains").into_string();

    let interchain = MockInterchainEnv::new(vec![("juno", &sender), ("osmosis", &sender)]);

    let juno = interchain.chain("juno").unwrap();
    let osmosis = interchain.chain("osmosis").unwrap();

    juno.set_balance(sender.clone(), vec![Coin::new(100000000000000, "juno")])
        .unwrap();

    let factory_juno = FactoryContract::new(juno.clone());
    let escrow_juno = EscrowContract::new(juno);
    let router_osmo = RouterContract::new(osmosis.clone());
    let virtual_balance_osmo = VirtualBalanceContract::new(osmosis);
    //Upload contracts
    escrow_juno.upload().unwrap();
    factory_juno.upload().unwrap();
    router_osmo.upload().unwrap();
    virtual_balance_osmo.upload().unwrap();

    let virtual_balance_code_id = virtual_balance_osmo.code_id().unwrap();
    let router_contract = "contract0".to_string();

    factory_juno
        .instantiate(
            &FactoryInstantiateMsg {
                router_contract,
                chain_uid: ChainUid::create("junouid".to_string()).unwrap(),
                escrow_code_id: 1,
                cw20_code_id: 2,
                is_native: false,
            },
            None,
            None,
        )
        .unwrap();
    // Upload vbalance contract

    router_osmo
        .instantiate(
            &RouterInstantiateMsg {
                vlp_code_id: 3,
                virtual_balance_code_id,
            },
            None,
            None,
        )
        .unwrap();

    // Set up channel from juno to osmosis
    let channel_receipt = interchain
        .create_contract_channel(&factory_juno, &router_osmo, "counter-1", None)
        .unwrap();

    // Set up channel from osmosis to juno
    let channel_receipt_osmosis = interchain
        .create_contract_channel(&router_osmo, &factory_juno, "counter-1", None)
        .unwrap();

    // After channel creation is complete, we get the channel id, which is necessary for ICA remote execution
    let juno_channel = channel_receipt
        .interchain_channel
        .get_chain("juno")
        .unwrap()
        .channel
        .unwrap();

    // Register factory should be the first execute msg from router
    let osmosis_channel = channel_receipt_osmosis
        .interchain_channel
        .get_chain("osmosis")
        .unwrap()
        .channel
        .unwrap();

    // Add a hub channel to factory-juno
    factory_juno
        .execute(
            &euclid::msgs::factory::ExecuteMsg::UpdateHubChannel {
                new_channel: osmosis_channel.to_string(),
            },
            None,
        )
        .unwrap();

    let juno_uid = ChainUid::create("junouid".to_string()).unwrap();
    let register_factory_request = router_osmo
        .execute(
            &euclid::msgs::router::ExecuteMsg::RegisterFactory {
                chain_uid: juno_uid.clone(),
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

    let packet_lifetime = interchain
        .wait_ibc("osmosis", register_factory_request)
        .unwrap();

    // For testing a successful outcome of the first packet sent out in the tx, you can use:
    if let IbcPacketOutcome::Success { ack, .. } = &packet_lifetime.packets[0].outcome {
        // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    } else {
        panic!("packet timed out");
        // There was a decode error or the packet timed out
        // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    };

    let all_chains: AllChainResponse = router_osmo
        .query(&euclid::msgs::router::QueryMsg::GetAllChains {})
        .unwrap();

    assert_eq!(
        all_chains.chains,
        vec![ChainResponse {
            chain: Chain {
                factory_chain_id: "juno".to_string(),
                factory: "contract0".to_string(),
                chain_type: euclid::chain::ChainType::Ibc(IbcChain {
                    from_hub_channel: "channel-1".to_string(),
                    from_factory_channel: "channel-1".to_string(),
                })
            },
            chain_uid: juno_uid
        }]
    );

    // Successful register factory //

    // let token_with_denom = TokenWithDenom {
    //     token: Token::create("juno".to_string()).unwrap(),
    //     token_type: TokenType::Native {
    //         denom: "juno".to_string(),
    //     },
    // };

    // // we should request to register escrow on factory-juno
    // let tx_response = factory_juno
    //     .execute(
    //         &euclid::msgs::factory::ExecuteMsg::RequestRegisterEscrow {
    //             token: token_with_denom.clone(),
    //             timeout: None,
    //         },
    //         None,
    //     )
    //     .unwrap();

    // // This broadcasts a transaction on the client
    // // It sends an IBC packet to the host
    // let amount = Uint128::from(100u128);

    // let packet_lifetime = interchain.wait_ibc("juno", tx_response).unwrap();

    // // For testing a successful outcome of the first packet sent out in the tx, you can use:
    // if let IbcPacketOutcome::Success { ack, .. } = &packet_lifetime.packets[0].outcome {
    //     // Packet has been successfully acknowledged and decoded, the transaction has gone through correctly
    // } else {
    //     panic!("packet timed out");
    //     // There was a decode error or the packet timed out
    //     // Else the packet timed-out, you may have a relayer error or something is wrong in your application
    // };

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
