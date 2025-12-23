use std::str::FromStr;

use cosmwasm_std::{from_json, to_json_binary, to_json_string, Addr, Binary, Event, HexBinary};
use cw_orch::{
    core::CwEnvError,
    mock::{cw_multi_test::App, MockBase},
    prelude::{ContractInstance, Environment},
};
use euclid::{
    chain::ChainUid,
    msgs::{
        self,
        factory::{QueryMsgFns, RegisterFactoryResponse},
        router::QueryMsgFns as RouterQueryFns,
    },
};
use euclid_ibc::{
    ack::AcknowledgementMsg,
    msg::{ChainIbcExecuteMsg, HubIbcExecuteMsg},
};
use factory::FactoryContract;
use k256::{ecdsa::SigningKey, elliptic_curve::NonZeroScalar};
use relayer::{
    verify::{MsgSignData, MsgSignDataMsg, MsgSignDataValue},
    ExecuteMsgFns as RelayerExecuteFns, MetaTransaction, MetaTransactionData,
};
use router::RouterContract;
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::chains::get_relayer;

pub fn relay_factory_send_packet(
    events: Vec<Event>,
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let send_packet_events = events
        .iter()
        .filter(|event| event.ty == "wasm-euclid-cosmos-send-packet")
        .collect::<Vec<_>>();

    for event in send_packet_events {
        let msg = event
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();

        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        let sequence = event
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();
        println!("relay_factory_send_packet: {:?}", sequence.value);

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        let hash = event
            .attributes
            .iter()
            .find(|attr| attr.key == "hash")
            .unwrap();

        let relayer_address = router
            .query_relayer_addresses()
            .unwrap()
            .relayer_addresses
            .first()
            .unwrap()
            .clone();
        println!("relay_factory_send_packet: {:?}", relayer_address);
        let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));
        let call_data = euclid::msgs::router::ExecuteMsg::CosmosReceivePacket {
            msg: msg_binary,
            chain_uid: chain_uid.clone(),
            sequence,
            hash: hash.value.clone(),
        };
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            router.address().unwrap(),
            format!("{}-{}-receive", sequence, **chain_uid),
            &router.environment().app.borrow(),
        );
        println!("signed_data: {:?}", signed_data);

        let response = relayer.execute_meta_transaction(signed_data)?;
        println!("response-mini: {:?}", response);
        responses.extend(response.events);
    }
    println!("responses: {:?}", responses);
    Ok(responses)
}

pub fn relay_factory_send_packet_evm(
    events: Vec<Event>,
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let send_packet_events = events
        .iter()
        .filter(|event| event.ty == "wasm-euclid-cosmos-send-packet")
        .collect::<Vec<_>>();

    for event in send_packet_events {
        let msg = event
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();

        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        let sequence = event
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();
        println!("relay_factory_send_packet: {:?}", sequence.value);

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        let hash = event
            .attributes
            .iter()
            .find(|attr| attr.key == "hash")
            .unwrap();

        let relayer_address = router
            .query_relayer_addresses()
            .unwrap()
            .relayer_addresses
            .first()
            .unwrap()
            .clone();
        println!("relay_factory_send_packet: {:?}", relayer_address);
        let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));
        let call_data = euclid::msgs::router::ExecuteMsg::EvmReceivePacket {
            msg: msg_binary,
            chain_uid: chain_uid.clone(),
            sequence,
            hash: hash.value.clone(),
        };
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            router.address().unwrap(),
            format!("{}-{}-receive", sequence, **chain_uid),
            &router.environment().app.borrow(),
        );

        let response = relayer.execute_meta_transaction(signed_data)?;
        responses.extend(response.events);
    }
    Ok(responses)
}

pub fn relay_router_send_packet(
    events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let send_packet_events = events
        .iter()
        .filter(|event| {
            event.ty == "wasm-euclid-cosmos-send-packet"
                || event.ty == "wasm-euclid-evm-send-packet"
        })
        .collect::<Vec<_>>();

    for event in send_packet_events {
        let msg = event
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();

        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        let sequence = event
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        println!("relay_router_send_packet sequence: {:?}", sequence);
        let hash = event
            .attributes
            .iter()
            .find(|attr| attr.key == "hash")
            .unwrap();

        let chain_uid = event
            .attributes
            .iter()
            .find(|attr| attr.key == "chain_uid")
            .unwrap();

        // If this send packet was not meant for the current factory, skip it
        if chain_uid.value != factory_chain_uid.to_string() {
            println!(
                "relay_router_send_packet: skipping packet for chain_uid: {:?}",
                chain_uid.value
            );
            continue;
        }
        let relayer_address = factory.get_relayer().unwrap();
        let relayer = get_relayer(
            factory.environment(),
            &Addr::unchecked(relayer_address.relayer_address),
        );

        let call_data = euclid::msgs::factory::ExecuteMsg::CosmosReceivePacket {
            msg: msg_binary,
            sequence,
            hash: hash.value.clone(),
        };

        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            factory.address().unwrap(),
            format!("{}-receive", sequence),
            &factory.environment().app.borrow(),
        );

        let response = relayer.execute_meta_transaction(signed_data)?;

        responses.extend(response.events);
    }
    Ok(responses)
}

pub fn relay_router_send_packet_evm(
    events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    println!("relay_router_send_packet_evm events: {:?}", events);
    let send_packet_events = events
        .iter()
        .filter(|event| event.ty == "wasm-euclid-evm-send-packet")
        .collect::<Vec<_>>();

    for event in send_packet_events {
        let msg = event
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();

        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        let sequence = event
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        println!("relay_router_send_packet sequence: {:?}", sequence);
        let hash = event
            .attributes
            .iter()
            .find(|attr| attr.key == "hash")
            .unwrap();

        let chain_uid = event
            .attributes
            .iter()
            .find(|attr| attr.key == "chain_uid")
            .unwrap();

        // If this send packet was not meant for the current factory, skip it
        if chain_uid.value != factory_chain_uid.to_string() {
            println!(
                "relay_router_send_packet: skipping packet for chain_uid: {:?}",
                chain_uid.value
            );
            continue;
        }
        let relayer_address = factory.get_relayer().unwrap();
        let relayer = get_relayer(
            factory.environment(),
            &Addr::unchecked(relayer_address.relayer_address),
        );

        let call_data = euclid::msgs::factory::ExecuteMsg::CosmosReceivePacket {
            msg: msg_binary,
            sequence,
            hash: hash.value.clone(),
        };

        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            factory.address().unwrap(),
            format!("{}-receive", sequence),
            &factory.environment().app.borrow(),
        );

        let response = relayer.execute_meta_transaction(signed_data)?;

        responses.extend(response.events);
    }
    Ok(responses)
}

pub fn relay_factory_ack_packet(
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
    events: Vec<Event>,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();
    println!("relay_factory_ack_packet events: {:?}", events);

    let write_ack_events = events
        .iter()
        .filter(|event| event.ty == "wasm-euclid-cosmos-write-acknowledgement")
        .collect::<Vec<_>>();

    for events in write_ack_events.chunks(2) {
        let msg = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();
        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        // if let ChainIbcExecuteMsg::Swap { .. } = msg_enum {
        //     panic!("relay_factory_ack_packet: {:?}", events);
        // }
        let sequence = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();

        let msg_enum = from_json::<ChainIbcExecuteMsg>(msg_binary.as_slice()).unwrap();
        println!("relay_factory_ack_packet msg: {:?}", msg_enum);
        println!("relay_factory_ack_packet sequence: {:?}", sequence);

        let hash = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "hash")
            .unwrap();

        let chain_uid = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "chain_uid")
            .unwrap();

        let ack = events[1]
            .attributes
            .iter()
            .find(|attr| attr.key == "ack")
            .unwrap();

        let ack_binary = Binary::from_base64(ack.value.as_str()).unwrap();

        // If this ack packet was not meant for the current factory, skip it
        if chain_uid.value != factory_chain_uid.to_string() {
            continue;
        }

        let relayer_address = factory.get_relayer().unwrap();
        let relayer = get_relayer(
            factory.environment(),
            &Addr::unchecked(relayer_address.relayer_address),
        );

        let call_data = euclid::msgs::factory::ExecuteMsg::CosmosReceiveAck {
            msg: msg_binary,
            sequence,
            hash: hash.value.clone(),
            ack: ack_binary,
        };

        println!("relay_factory_ack_packet ack: {:?}", ack.value);

        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            factory.address().unwrap(),
            format!("{}-ack", sequence),
            &factory.environment().app.borrow(),
        );

        let response = relayer.execute_meta_transaction(signed_data)?;

        responses.extend(response.events);
    }
    Ok(responses)
}

pub fn relay_factory_ack_packet_evm(
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
    events: Vec<Event>,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();
    println!("relay_factory_ack_packet events: {:?}", events);

    let write_ack_events = events
        .iter()
        .filter(|event| event.ty == "wasm-euclid-evm-write-acknowledgement")
        .collect::<Vec<_>>();

    for events in write_ack_events.chunks(2) {
        let msg = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();
        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        // if let ChainIbcExecuteMsg::Swap { .. } = msg_enum {
        //     panic!("relay_factory_ack_packet: {:?}", events);
        // }
        let sequence = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();

        let msg_enum = from_json::<ChainIbcExecuteMsg>(msg_binary.as_slice()).unwrap();
        println!("relay_factory_ack_packet msg: {:?}", msg_enum);
        println!("relay_factory_ack_packet sequence: {:?}", sequence);

        let hash = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "hash")
            .unwrap();

        let chain_uid = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "chain_uid")
            .unwrap();

        let ack = events[1]
            .attributes
            .iter()
            .find(|attr| attr.key == "ack")
            .unwrap();

        let ack_binary = Binary::from_base64(ack.value.as_str()).unwrap();

        // If this ack packet was not meant for the current factory, skip it
        if chain_uid.value != factory_chain_uid.to_string() {
            continue;
        }

        let relayer_address = factory.get_relayer().unwrap();
        let relayer = get_relayer(
            factory.environment(),
            &Addr::unchecked(relayer_address.relayer_address),
        );

        let call_data = euclid::msgs::factory::ExecuteMsg::CosmosReceiveAck {
            msg: msg_binary,
            sequence,
            hash: hash.value.clone(),
            ack: ack_binary,
        };

        println!("relay_factory_ack_packet ack: {:?}", ack.value);

        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            factory.address().unwrap(),
            format!("{}-ack", sequence),
            &factory.environment().app.borrow(),
        );

        let response = relayer.execute_meta_transaction(signed_data)?;

        responses.extend(response.events);
    }
    Ok(responses)
}

pub fn relay_router_ack_packet(
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
    events: Vec<Event>,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let write_ack_events = events
        .iter()
        .filter(|event| event.ty == "wasm-euclid-cosmos-write-acknowledgement")
        .collect::<Vec<_>>();

    for events in write_ack_events.chunks(2) {
        let msg = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();

        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        let sequence = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();
        println!("relay_router_ack_packet: {:?}", sequence.value);

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        let hash = events[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "hash")
            .unwrap();

        let ack = events[1]
            .attributes
            .iter()
            .find(|attr| attr.key == "ack")
            .unwrap();

        let ack_binary = Binary::from_base64(ack.value.as_str()).unwrap();

        let relayer_address = router
            .query_relayer_addresses()
            .unwrap()
            .relayer_addresses
            .first()
            .unwrap()
            .clone();
        let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

        let call_data = euclid::msgs::router::ExecuteMsg::CosmosReceiveAck {
            msg: msg_binary,
            chain_uid: chain_uid.clone(),
            sequence,
            hash: hash.value.clone(),
            ack: ack_binary,
        };

        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            router.address().unwrap(),
            format!("{}-{}-ack", sequence, **chain_uid),
            &router.environment().app.borrow(),
        );

        println!("signed_data: {:?}", signed_data);

        let response = relayer.execute_meta_transaction(signed_data);
        println!("response: {:?}", response);

        responses.extend(response.unwrap().events);
    }
    Ok(responses)
}

pub fn relay_router_ack_packet_evm(
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    // let write_ack_events = events
    //     .iter()
    //     .filter(|event| event.ty == "wasm-euclid-evm-write-acknowledgement")
    //     .collect::<Vec<_>>();

    // // for events in write_ack_events.chunks(2) {
    // let msg = events[0]
    //     .attributes
    //     .iter()
    //     .find(|attr| attr.key == "msg")
    //     .unwrap();

    let ack = AcknowledgementMsg::Ok(RegisterFactoryResponse {
        factory_address: "factory_address".to_string(),
        chain_id: "ethereum".to_string(),
    });

    let evm_receive_packet_msg = msgs::router::ExecuteMsg::EvmReceiveAck {
        msg: to_json_binary(&HubIbcExecuteMsg::RegisterFactory {
            chain_uid: chain_uid.clone(),
            tx_id: "".to_string(),
        })
        .unwrap(),
        chain_uid: chain_uid.clone(),
        sequence: 0,
        hash: "".to_string(),
        ack: to_json_binary(&ack).unwrap(),
    };

    let msg_binary = to_json_binary(&evm_receive_packet_msg).unwrap();
    // let sequence = events[0]
    //     .attributes
    //     .iter()
    //     .find(|attr| attr.key == "sequence")
    //     .unwrap();
    // println!("relay_router_ack_packet: {:?}", sequence.value);

    // let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
    // let hash = events[0]
    //     .attributes
    //     .iter()
    //     .find(|attr| attr.key == "hash")
    //     .unwrap();

    // let ack = events[1]
    //     .attributes
    //     .iter()
    //     .find(|attr| attr.key == "ack")
    //     .unwrap();

    let ack_binary = to_json_binary(&ack).unwrap();

    let relayer_address = router
        .query_relayer_addresses()
        .unwrap()
        .relayer_addresses
        .first()
        .unwrap()
        .clone();
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    let call_data = euclid::msgs::router::ExecuteMsg::EvmReceiveAck {
        msg: to_json_binary(&HubIbcExecuteMsg::RegisterFactory {
            chain_uid: chain_uid.clone(),
            tx_id: "".to_string(),
        })
        .unwrap(),
        chain_uid: chain_uid.clone(),
        sequence: 0,
        hash: "".to_string(),
        ack: ack_binary,
    };

    let signed_data = sign_relay_messsage(
        to_json_binary(&call_data).unwrap(),
        router.address().unwrap(),
        format!("{}-{}-ack", 0, **chain_uid),
        &router.environment().app.borrow(),
    );

    println!("signed_data: {:?}", signed_data);

    let response = relayer.execute_meta_transaction(signed_data);
    println!("response: {:?}", response);

    responses.extend(response.unwrap().events);
    // }
    Ok(responses)
}

pub fn relay_factory_router_factory(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    factory_chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    println!("relay_factory_router_factory1");
    let ack_events = relay_factory_send_packet(send_events, router, factory_chain_uid)?;
    println!("relay_factory_router_factory2");
    relay_factory_ack_packet(factory, factory_chain_uid, ack_events.clone())?;
    println!("relay_factory_router_factory3");
    Ok(ack_events)
}

pub fn relay_factory_router_factory_evm(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    factory_chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let ack_events = relay_factory_send_packet_evm(send_events, router, factory_chain_uid)?;
    relay_factory_ack_packet_evm(factory, factory_chain_uid, ack_events.clone())?;
    Ok(ack_events)
}

#[allow(dead_code)]
pub fn relay_router_factory_router(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
    router: &RouterContract<MockBase>,
) -> Result<Vec<Event>, CwEnvError> {
    let ack_events = relay_router_send_packet(send_events, factory, factory_chain_uid)?;
    relay_router_ack_packet(router, factory_chain_uid, ack_events.clone())?;
    Ok(ack_events)
}

pub fn get_random_private_key(seed: &str) -> String {
    let mut new_private_key = HexBinary::from(seed.as_bytes()).to_string();
    while new_private_key.len() < 64 {
        new_private_key = HexBinary::from(new_private_key.as_bytes()).to_string();
    }
    new_private_key.truncate(64);
    new_private_key
}

pub fn get_signer_key() -> (SigningKey, Binary) {
    let pk = "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369";

    get_signer_key_from_pk(pk)
}
pub fn get_signer_key_from_pk(pk: &str) -> (SigningKey, Binary) {
    let scalar = NonZeroScalar::from_str(pk).unwrap();

    let signer_key = SigningKey::from(scalar);
    let pubkey = signer_key
        .verifying_key()
        .to_encoded_point(true) // true = compressed format (33 bytes) for Cosmos
        .as_bytes()
        .to_vec();

    let pubkey_binary = Binary::from(pubkey);
    (signer_key, pubkey_binary)
}

pub fn get_signer_key_from_pk_evm(pk: &str) -> (SigningKey, Binary) {
    let scalar = NonZeroScalar::from_str(pk).unwrap();

    let signer_key = SigningKey::from(scalar);
    let pubkey = signer_key
        .verifying_key()
        .to_encoded_point(false) // false = uncompressed format (65 bytes) for EVM
        .as_bytes()
        .to_vec();

    let pubkey_binary = Binary::from(pubkey);
    (signer_key, pubkey_binary)
}

pub fn sign_relay_messsage(
    call_data: Binary,
    target: Addr,
    nonce: String,
    app: &App,
) -> MetaTransaction {
    let meta_tx_data = MetaTransactionData {
        call_data,
        expiry: app.block_info().time.plus_seconds(60).seconds(),
        nonce,
        target,
    };

    let msg = MsgSignDataMsg::new(MsgSignDataValue::new(
        to_json_binary(&meta_tx_data).unwrap(),
        format!("relayer_{}", app.block_info().chain_id),
    ));
    let msg = MsgSignData::new(vec![msg]);
    let msg = to_json_string(&msg).unwrap();
    let message_digest = Sha256::new().chain(msg.as_bytes());

    let (secret_key, _) = get_signer_key();
    let signature = secret_key
        .sign_digest_recoverable(message_digest)
        .unwrap()
        .0;
    MetaTransaction {
        data: msg,
        signature: Binary::from(signature.to_vec()),
    }
}
