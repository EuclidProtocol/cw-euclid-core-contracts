use std::str::FromStr;

use cosmwasm_std::{to_json_binary, to_json_string, Addr, Binary, Event, HexBinary};
use cw_orch::{
    core::CwEnvError,
    mock::{cw_multi_test::App, MockBase},
    prelude::{ContractInstance, Environment},
};
use euclid::{
    chain::ChainUid,
    events::{EUCLID_SEND_PACKET_EVENT, EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT},
    msgs::{
        factory::{QueryMsgFns as FactoryQueryFns, RegisterFactoryResponse},
        router::{
            QueryMsgFns as RouterQueryFns, RegisterFactoryChainEvm, RegisterFactoryChainType,
        },
    },
};
use euclid_ibc::{ack::AcknowledgementMsg, factory_ibc::FactoryCrossChainExecuteMsg};
use factory::FactoryContract;
use k256::{ecdsa::SigningKey, elliptic_curve::NonZeroScalar};
use relayer::{
    ExecuteMsgFns as RelayerExecuteFns, MetaTransaction as RelayerMetaTransaction,
    MetaTransactionData as RelayerMetaTransactionData, ValidatorSignature,
};
use router::RouterContract;
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::chains::get_relayer;

pub fn relay_factory_send_packet(
    events: Vec<Event>,
    router: &RouterContract<MockBase>,
) -> Result<Vec<Event>, CwEnvError> {
    relay_factory_send_packet_inner(events, router)
}

fn relay_factory_send_packet_inner(
    events: Vec<Event>,
    router: &RouterContract<MockBase>,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let send_packets = extract_send_packet_events(&events);

    println!("Relay factory send packets to router:");
    println!("packet count: {:?}", send_packets.len());

    let relayer_address = router.query_relayer_addresses().unwrap().relayer_contract;
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    for packet in send_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        let call_data = euclid::msgs::router::ExecuteMsg::ReceivePacket {
            msg: packet.msg,
            sequence: packet.sequence,
            source_port: packet.source_port.clone(),
            destination_port: packet.destination_port.clone(),
            timeout: None,
        };
        let source_chain_uid = packet.source_port.split('.').next().unwrap();
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            router.address().unwrap(),
            format!(
                "{}-{}-{}-receive",
                packet.source_port, packet.destination_port, packet.sequence,
            ),
            &router.environment().app.borrow(),
            source_chain_uid,
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

    let send_packets = extract_send_packet_events(&events);

    println!("Relay router send packets to factory:");
    println!("packet count: {:?}", send_packets.len());

    let relayer_address = factory.get_state().unwrap().relayer_contract;
    let relayer = get_relayer(factory.environment(), &Addr::unchecked(relayer_address));

    for packet in send_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        let expected_destination_port = format!(
            "{}.{}",
            factory_chain_uid.as_str(),
            factory.address().unwrap()
        );
        if packet.destination_port != expected_destination_port {
            println!(
                "relay_router_send_packet: skipping packet for destination_port: {:?}",
                packet.destination_port
            );
            continue;
        }

        let call_data = euclid::msgs::factory::ExecuteMsg::ReceivePacket {
            msg: packet.msg,
            sequence: packet.sequence,
            source_port: packet.source_port.clone(),
            destination_port: packet.destination_port.clone(),
            timeout: None,
        };

        let source_chain_uid = packet.source_port.split('.').next().unwrap();
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            factory.address().unwrap(),
            format!(
                "{}-{}-{}-receive",
                packet.source_port, packet.destination_port, packet.sequence
            ),
            &factory.environment().app.borrow(),
            source_chain_uid,
        );

        let response = relayer.execute_meta_transaction(signed_data)?;

        responses.extend(response.events);
    }
    Ok(responses)
}

pub fn relay_factory_ack_packet(
    factory: &FactoryContract<MockBase>,
    events: Vec<Event>,
    chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let write_ack_packets = extract_ack_packet_events(&events);
    println!("Relay factory acknowledge packets:");
    println!("packet count: {:?}", write_ack_packets.len());
    let relayer_address = factory.get_state().unwrap().relayer_contract;
    let relayer = get_relayer(factory.environment(), &Addr::unchecked(relayer_address));

    let destination_port = format!("{}.{}", chain_uid.as_str(), factory.address().unwrap());
    for packet in write_ack_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        println!("Ack packet: {:?}", packet.ack.to_base64());

        if packet.destination_port != destination_port {
            println!(
                "Skipping packet for destination_port: {:?}",
                packet.destination_port
            );
            continue;
        }

        let call_data = euclid::msgs::factory::ExecuteMsg::AcknowledgePacket {
            source_port: packet.source_port.clone(),
            destination_port: packet.destination_port.clone(),
            msg: packet.msg,
            sequence: packet.sequence,
            ack: packet.ack,
        };

        let source_chain_uid = packet.source_port.split('.').next().unwrap();
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            factory.address().unwrap(),
            format!(
                "{}-{}-{}-ack",
                packet.source_port, packet.destination_port, packet.sequence
            ),
            &factory.environment().app.borrow(),
            source_chain_uid,
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

    let write_ack_packets = extract_ack_packet_events(&events);
    println!("Relay router acknowledge packets:");
    println!("packet count: {:?}", write_ack_packets.len());
    let relayer_address = router.query_relayer_addresses().unwrap().relayer_contract;
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    for packet in write_ack_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        println!("Ack packet: {:?}", packet.ack.to_base64());
        let call_data = euclid::msgs::router::ExecuteMsg::AcknowledgePacket {
            source_port: packet.source_port.clone(),
            destination_port: packet.destination_port.clone(),
            msg: packet.msg,
            sequence: packet.sequence,
            ack: packet.ack,
        };
        let source_chain_uid = packet.source_port.split('.').next().unwrap();
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            router.address().unwrap(),
            format!(
                "{}-{}-{}-ack",
                packet.source_port, packet.destination_port, packet.sequence
            ),
            &router.environment().app.borrow(),
            source_chain_uid,
        );

        let response = relayer.execute_meta_transaction(signed_data);

        responses.extend(response.unwrap().events);
    }
    Ok(responses)
}

pub fn ack_register_factory_evm(
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
    factory_address: &str,
    chain_id: &str,
    tx_id: &str,
    sequence: u128,
) -> Result<Vec<Event>, CwEnvError> {
    let ack = AcknowledgementMsg::Ok(RegisterFactoryResponse {
        factory_address: factory_address.to_string(),
        chain_id: chain_id.to_string(),
    });

    let ack_binary = to_json_binary(&ack).unwrap();

    let relayer_address = router.query_relayer_addresses().unwrap().relayer_contract;
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    let evm_port = format!("{}.{}", chain_uid.as_str(), factory_address).to_string();
    let vsl_port = format!("vsl.{}", router.address().unwrap()).to_string();

    let call_data = euclid::msgs::router::ExecuteMsg::AcknowledgePacket {
        source_port: evm_port.clone(),
        destination_port: vsl_port.clone(),
        msg: to_json_binary(&FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid: chain_uid.clone(),
            chain_type: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
                factory_address: factory_address.to_string(),
                factory_chain_id: chain_id.to_string(),
            }),
            tx_id: tx_id.to_string(),
        })
        .unwrap(),
        sequence,
        ack: ack_binary,
    };
    let source_chain_uid = chain_uid.as_str();
    let signed_data = sign_relay_messsage(
        to_json_binary(&call_data).unwrap(),
        router.address().unwrap(),
        format!("{}-{}-{}-ack", evm_port, vsl_port, 0,),
        &router.environment().app.borrow(),
        source_chain_uid,
    );

    let response = relayer.execute_meta_transaction(signed_data);

    Ok(response.unwrap().events)
}

pub fn relay_factory_router_factory(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let ack_events = relay_factory_send_packet(send_events, router)?;
    relay_factory_ack_packet(factory, ack_events.clone(), chain_uid)?;
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
    source_chain_uid: &str,
) -> RelayerMetaTransaction {
    let meta_tx_data = RelayerMetaTransactionData {
        call_data,
        nonce,
        target,
    };
    let expiry = app.block_info().time.plus_seconds(60).seconds();
    let msg = to_json_string(&meta_tx_data).unwrap();
    let expiry_call_data = format!(
        "{msg},{expiry},{source_chain_uid}",
        msg = msg,
        expiry = expiry,
        source_chain_uid = source_chain_uid
    );
    let message_digest = Sha256::new().chain(expiry_call_data.as_bytes());

    let (secret_key, pubkey) = get_signer_key();
    let signature = secret_key
        .sign_digest_recoverable(message_digest)
        .unwrap()
        .0;
    let admin_signature = Binary::from(signature.to_vec());
    RelayerMetaTransaction {
        data: msg,
        expiry: app.block_info().time.plus_seconds(60).seconds(),
        admin_signature: admin_signature.clone(),
        validator_signatures: vec![ValidatorSignature {
            pubkey,
            signature: admin_signature,
            expiry: app.block_info().time.plus_seconds(60).seconds(),
        }],
        chain_uid: ChainUid::create(source_chain_uid.to_string()).unwrap(),
    }
}

pub struct SendPacketEvent {
    pub msg: Binary,
    pub sequence: u128,
    pub source_port: String,
    pub destination_port: String,
}
pub fn extract_send_packet_events(events: &[Event]) -> Vec<SendPacketEvent> {
    let mut send_packet_events = vec![];

    let send_packet_event_type = format!("wasm-{}", EUCLID_SEND_PACKET_EVENT);

    let related_events = events
        .iter()
        .filter(|event| event.ty == send_packet_event_type)
        .collect::<Vec<_>>();

    for event in related_events {
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
        let source_port = event
            .attributes
            .iter()
            .find(|attr| attr.key == "source_port")
            .unwrap();
        let destination_port = event
            .attributes
            .iter()
            .find(|attr| attr.key == "destination_port")
            .unwrap();
        send_packet_events.push(SendPacketEvent {
            msg: msg_binary,
            sequence,
            source_port: source_port.value.clone(),
            destination_port: destination_port.value.clone(),
        });
    }
    send_packet_events
}

pub struct AckPacketEvent {
    msg: Binary,
    ack: Binary,
    sequence: u128,
    source_port: String,
    destination_port: String,
}
pub fn extract_ack_packet_events(events: &[Event]) -> Vec<AckPacketEvent> {
    let mut ack_packet_events = vec![];

    let ack_packet_event_type = format!("wasm-{}", EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT);

    let related_events = events
        .iter()
        .filter(|event| event.ty == ack_packet_event_type)
        .collect::<Vec<_>>();

    for event in related_events.chunks(2) {
        let msg = event[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "msg")
            .unwrap();
        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();

        let ack = event[1]
            .attributes
            .iter()
            .find(|attr| attr.key == "ack")
            .unwrap();
        let ack_binary = Binary::from_base64(ack.value.as_str()).unwrap();
        let sequence = event[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "sequence")
            .unwrap();
        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        let source_port = event[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "source_port")
            .unwrap();
        let destination_port = event[0]
            .attributes
            .iter()
            .find(|attr| attr.key == "destination_port")
            .unwrap();
        ack_packet_events.push(AckPacketEvent {
            msg: msg_binary,
            ack: ack_binary,
            sequence,
            source_port: source_port.value.clone(),
            destination_port: destination_port.value.clone(),
        });
    }
    ack_packet_events
}
