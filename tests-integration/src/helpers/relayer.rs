#![cfg(not(target_arch = "wasm32"))]

use std::str::FromStr;

use cosmwasm_std::{to_json_binary, to_json_string, Addr, Binary, Event, HexBinary};
use cw_multi_test::BasicApp;
use euclid::{
    chain::ChainUid,
    events::{EUCLID_SEND_PACKET_EVENT, EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT},
    msgs::factory::RegisterFactoryResponse,
};
use euclid_ibc::{ack::AcknowledgementMsg, factory_ibc::FactoryCrossChainExecuteMsg};
use k256::{ecdsa::SigningKey, elliptic_curve::NonZeroScalar};
use relayer::{
    MetaTransaction as RelayerMetaTransaction, MetaTransactionData as RelayerMetaTransactionData,
    ValidatorSignature,
};
use sha2::{digest::Update, Digest, Sha256};

use crate::helpers::app::EuclidApp;
use crate::helpers::multi_chain::MultiChainEnv;

// ---------------------------------------------------------------------------
// Single-app relay primitives
// ---------------------------------------------------------------------------

/// Relay packets from factory → router. Operates only on the router chain app.
pub fn relay_factory_send_packet(
    events: Vec<Event>,
    router_addr: &Addr,
    router_app: &mut EuclidApp,
) -> Result<Vec<Event>, anyhow::Error> {
    let mut responses = Vec::new();
    let send_packets = extract_send_packet_events(&events);

    println!("Relay factory send packets to router:");
    println!("send packet count: {:?}", send_packets.len());

    let relayer_state: euclid::msgs::router::QueryRelayerAddressesResponse = router_app.query(
        router_addr,
        &euclid::msgs::router::QueryMsg::QueryRelayerAddresses {},
    );
    let relayer_addr = relayer_state.relayer_contract;

    for packet in send_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        let call_data = euclid::msgs::router::ExecuteMsg::ReceivePacket {
            msg: packet.msg,
            sequence: packet.sequence,
            source_port: packet.source_port.clone(),
            destination_port: packet.destination_port.clone(),
            timeout: packet.timeout,
        };
        let source_chain_uid = packet.source_port.split('.').next().unwrap();
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            router_addr.clone(),
            format!(
                "{}-{}-{}-receive",
                packet.source_port, packet.destination_port, packet.sequence,
            ),
            router_app.app(),
            source_chain_uid,
        );

        let sender = router_app.sender();
        let response = router_app.execute(
            &sender,
            &relayer_addr,
            &relayer::ExecuteMsg::ExecuteMetaTransaction(signed_data),
            &[],
        );
        responses.extend(response.events);
    }
    Ok(responses)
}

/// Relay packets from router → factory. Operates only on the factory chain app.
pub fn relay_router_send_packet(
    events: Vec<Event>,
    factory_addr: &Addr,
    factory_chain_uid: &ChainUid,
    factory_app: &mut EuclidApp,
) -> Result<Vec<Event>, anyhow::Error> {
    let mut responses = Vec::new();
    let send_packets = extract_send_packet_events(&events);

    println!("Relay router send packets to factory:");
    println!("send packet count: {:?}", send_packets.len());

    let factory_state: euclid::msgs::factory::StateResponse =
        factory_app.query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
    let relayer_addr = factory_state.relayer_contract;

    for packet in send_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        let expected_destination_port = format!("{}.{}", factory_chain_uid.as_str(), factory_addr);
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
            timeout: packet.timeout,
        };

        let source_chain_uid = packet.source_port.split('.').next().unwrap();
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).unwrap(),
            factory_addr.clone(),
            format!(
                "{}-{}-{}-receive",
                packet.source_port, packet.destination_port, packet.sequence
            ),
            factory_app.app(),
            source_chain_uid,
        );

        let sender = factory_app.sender();
        let response = factory_app.execute(
            &sender,
            &relayer_addr,
            &relayer::ExecuteMsg::ExecuteMetaTransaction(signed_data),
            &[],
        );
        responses.extend(response.events);
    }
    Ok(responses)
}

/// Relay acknowledgement packets back to factory. Operates only on the factory chain app.
/// Returns `Err` if any ack contains an error response.
pub fn relay_factory_ack_packet(
    factory_addr: &Addr,
    events: Vec<Event>,
    chain_uid: &ChainUid,
    factory_app: &mut EuclidApp,
) -> Result<Vec<Event>, anyhow::Error> {
    let mut responses = Vec::new();
    let write_ack_packets = extract_ack_packet_events(&events);
    println!("Relay factory acknowledge packets:");
    println!("write ack packet count: {:?}", write_ack_packets.len());

    let factory_state: euclid::msgs::factory::StateResponse =
        factory_app.query(factory_addr, &euclid::msgs::factory::QueryMsg::GetState {});
    let relayer_addr = factory_state.relayer_contract;

    let destination_port = format!("{}.{}", chain_uid.as_str(), factory_addr);
    let mut error_ack: Option<String> = None;
    for packet in write_ack_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Ack packet: {:?}", packet.ack.to_base64());

        if let Some(err) = extract_ack_error(&packet.ack) {
            error_ack = Some(err);
        }

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
            factory_addr.clone(),
            format!(
                "{}-{}-{}-ack",
                packet.source_port, packet.destination_port, packet.sequence
            ),
            factory_app.app(),
            source_chain_uid,
        );

        let sender = factory_app.sender();
        let response = factory_app.execute(
            &sender,
            &relayer_addr,
            &relayer::ExecuteMsg::ExecuteMetaTransaction(signed_data),
            &[],
        );
        responses.extend(response.events);
    }

    if let Some(err) = error_ack {
        return Err(anyhow::anyhow!("{}", err));
    }

    Ok(responses)
}

fn extract_ack_error(ack: &Binary) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct MaybeError {
        #[serde(default)]
        error: Option<String>,
    }
    cosmwasm_std::from_json::<MaybeError>(ack)
        .ok()
        .and_then(|a| a.error)
}

/// Relay acknowledgement packets back to router. Operates only on the router chain app.
pub fn relay_router_ack_packet(
    router_addr: &Addr,
    _chain_uid: &ChainUid,
    events: Vec<Event>,
    router_app: &mut EuclidApp,
) -> Result<Vec<Event>, anyhow::Error> {
    let mut responses = Vec::new();
    let write_ack_packets = extract_ack_packet_events(&events);
    println!("Relay router acknowledge packets:");
    println!("write ack packet count: {:?}", write_ack_packets.len());

    let relayer_state: euclid::msgs::router::QueryRelayerAddressesResponse = router_app.query(
        router_addr,
        &euclid::msgs::router::QueryMsg::QueryRelayerAddresses {},
    );
    let relayer_addr = relayer_state.relayer_contract;

    for packet in write_ack_packets {
        println!("Packet sequence: {:?}", packet.sequence);
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
            router_addr.clone(),
            format!(
                "{}-{}-{}-ack",
                packet.source_port, packet.destination_port, packet.sequence
            ),
            router_app.app(),
            source_chain_uid,
        );

        let sender = router_app.sender();
        let response = router_app.execute(
            &sender,
            &relayer_addr,
            &relayer::ExecuteMsg::ExecuteMetaTransaction(signed_data),
            &[],
        );
        responses.extend(response.events);
    }
    Ok(responses)
}

/// Simulates EVM factory registration acknowledgement.
pub fn ack_register_factory_evm(
    router_addr: &Addr,
    router_app: &mut EuclidApp,
    chain_uid: &ChainUid,
    factory_address: &str,
    chain_id: &str,
    tx_id: &str,
    sequence: u128,
) -> Result<Vec<Event>, anyhow::Error> {
    let ack = AcknowledgementMsg::Ok(RegisterFactoryResponse {
        factory_address: factory_address.to_string(),
        chain_id: chain_id.to_string(),
    });

    let ack_binary = to_json_binary(&ack).unwrap();

    let relayer_state: euclid::msgs::router::QueryRelayerAddressesResponse = router_app.query(
        router_addr,
        &euclid::msgs::router::QueryMsg::QueryRelayerAddresses {},
    );
    let relayer_addr = relayer_state.relayer_contract;

    let evm_port = format!("{}.{}", chain_uid.as_str(), factory_address);
    let vsl_port = format!("vsl.{}", router_addr);

    let call_data = euclid::msgs::router::ExecuteMsg::AcknowledgePacket {
        source_port: evm_port.clone(),
        destination_port: vsl_port.clone(),
        msg: to_json_binary(&FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid: chain_uid.clone(),
            chain_type: euclid::msgs::router::RegisterFactoryChainType::Evm(
                euclid::msgs::router::RegisterFactoryChainEvm {
                    factory_address: factory_address.to_string(),
                    factory_chain_id: chain_id.to_string(),
                },
            ),
            tx_id: tx_id.to_string(),
        })
        .unwrap(),
        sequence,
        ack: ack_binary,
    };
    let source_chain_uid = chain_uid.as_str();
    let signed_data = sign_relay_messsage(
        to_json_binary(&call_data).unwrap(),
        router_addr.clone(),
        format!("{}-{}-{}-ack", evm_port, vsl_port, 0,),
        router_app.app(),
        source_chain_uid,
    );

    let sender = router_app.sender();
    let response = router_app.execute(
        &sender,
        &relayer_addr,
        &relayer::ExecuteMsg::ExecuteMetaTransaction(signed_data),
        &[],
    );
    Ok(response.events)
}

// ---------------------------------------------------------------------------
// Multi-chain compound relay helpers (use MultiChainEnv to avoid aliasing)
// ---------------------------------------------------------------------------

/// Factory → Router → Factory: send + ack round-trip.
pub fn relay_factory_router_factory(
    send_events: Vec<Event>,
    factory_chain_id: &str,
    factory_addr: &Addr,
    factory_chain_uid: &ChainUid,
    router_chain_id: &str,
    router_addr: &Addr,
    env: &mut MultiChainEnv,
) -> Result<Vec<Event>, anyhow::Error> {
    // Step 1: relay factory → router (needs router app only)
    let ack_events =
        relay_factory_send_packet(send_events, router_addr, env.chain_mut(router_chain_id))?;
    // Step 2: relay ack → factory (needs factory app only; safe even if same chain_id)
    relay_factory_ack_packet(
        factory_addr,
        ack_events.clone(),
        factory_chain_uid,
        env.chain_mut(factory_chain_id),
    )?;
    Ok(ack_events)
}

/// Router → Factory → Router: send + ack round-trip.
#[allow(dead_code)]
pub fn relay_router_factory_router(
    send_events: Vec<Event>,
    factory_chain_id: &str,
    factory_addr: &Addr,
    factory_chain_uid: &ChainUid,
    router_chain_id: &str,
    router_addr: &Addr,
    env: &mut MultiChainEnv,
) -> Result<Vec<Event>, anyhow::Error> {
    let ack_events = relay_router_send_packet(
        send_events,
        factory_addr,
        factory_chain_uid,
        env.chain_mut(factory_chain_id),
    )?;
    relay_router_ack_packet(
        router_addr,
        factory_chain_uid,
        ack_events.clone(),
        env.chain_mut(router_chain_id),
    )?;
    Ok(ack_events)
}

// ---------------------------------------------------------------------------
// Crypto helpers
// ---------------------------------------------------------------------------

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
        .to_encoded_point(true)
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
        .to_encoded_point(false)
        .as_bytes()
        .to_vec();
    let pubkey_binary = Binary::from(pubkey);
    (signer_key, pubkey_binary)
}

pub fn sign_relay_messsage(
    call_data: Binary,
    target: Addr,
    nonce: String,
    app: &BasicApp,
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

// ---------------------------------------------------------------------------
// Event extraction
// ---------------------------------------------------------------------------

pub struct SendPacketEvent {
    pub msg: Binary,
    pub sequence: u128,
    pub source_port: String,
    pub destination_port: String,
    pub timeout: u64,
}

pub fn extract_send_packet_events(events: &[Event]) -> Vec<SendPacketEvent> {
    let mut send_packet_events = vec![];
    let send_packet_event_type = format!("wasm-{}", EUCLID_SEND_PACKET_EVENT);

    for event in events.iter().filter(|e| e.ty == send_packet_event_type) {
        let msg = event.attributes.iter().find(|a| a.key == "msg").unwrap();
        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();
        let sequence = event
            .attributes
            .iter()
            .find(|a| a.key == "sequence")
            .unwrap();
        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        let source_port = event
            .attributes
            .iter()
            .find(|a| a.key == "source_port")
            .unwrap();
        let destination_port = event
            .attributes
            .iter()
            .find(|a| a.key == "destination_port")
            .unwrap();
        let timeout = event
            .attributes
            .iter()
            .find(|a| a.key == "timeout")
            .unwrap();
        let timeout = str::parse::<u64>(timeout.value.as_str()).unwrap();
        send_packet_events.push(SendPacketEvent {
            msg: msg_binary,
            sequence,
            source_port: source_port.value.clone(),
            destination_port: destination_port.value.clone(),
            timeout,
        });
    }
    send_packet_events
}

pub struct AckPacketEvent {
    pub msg: Binary,
    pub ack: Binary,
    pub sequence: u128,
    pub source_port: String,
    pub destination_port: String,
}

pub fn extract_ack_packet_events(events: &[Event]) -> Vec<AckPacketEvent> {
    let mut ack_packet_events = vec![];
    let ack_packet_event_type = format!("wasm-{}", EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT);

    let related_events: Vec<_> = events
        .iter()
        .filter(|e| e.ty == ack_packet_event_type)
        .collect();

    for event in related_events.chunks(2) {
        let msg = event[0].attributes.iter().find(|a| a.key == "msg").unwrap();
        let msg_binary = Binary::from_base64(msg.value.as_str()).unwrap();

        let ack = event[1].attributes.iter().find(|a| a.key == "ack").unwrap();
        let ack_binary = Binary::from_base64(ack.value.as_str()).unwrap();
        let sequence = event[0]
            .attributes
            .iter()
            .find(|a| a.key == "sequence")
            .unwrap();
        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        let source_port = event[0]
            .attributes
            .iter()
            .find(|a| a.key == "source_port")
            .unwrap();
        let destination_port = event[0]
            .attributes
            .iter()
            .find(|a| a.key == "destination_port")
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
