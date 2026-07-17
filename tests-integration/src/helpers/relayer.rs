use std::str::FromStr;

use cosmwasm_std::{to_json_binary, to_json_string, Addr, Binary, Event, HexBinary};
use cw_orch::{
    core::CwEnvError,
    mock::{cw_multi_test::App, MockBase},
    prelude::{ContractInstance, Environment},
};
use euclid::{
    chain::ChainUid,
    events::{EUCLID_SEND_PACKET_ENCODED_EVENT, EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT},
    msgs::{
        factory::{QueryMsgFns as FactoryQueryFns, RegisterFactoryResponse},
        router::{
            QueryMsgFns as RouterQueryFns, RegisterFactoryChainEvm, RegisterFactoryChainType,
        },
    },
};
use euclid_ibc::wire::{
    envelope::{factory::FactoryReceiveMsg, AcknowledgementMsg},
    msgs::RegisterFactorySendMsg,
};
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
    println!("send packet count: {:?}", send_packets.len());

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
            timeout: packet.timeout,
            encoding: packet.encoding,
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

/// Re-deliver a previously-relayed factory→router packet using a fresh
/// relayer-level nonce so the relayer's own meta-tx nonce dedup does NOT
/// fire. This lets a test exercise the router's contract-level dedup paths
/// (`CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS`, `TxAlreadyExist`) directly.
///
/// `relayer_nonce_suffix` is appended to the relayer nonce string to make
/// it distinct from the original delivery. Returns the `Result` from
/// `execute_meta_transaction` for the (single) duplicate packet, so the
/// caller can assert on the contract-level error.
pub fn redeliver_factory_send_packet(
    events: Vec<Event>,
    router: &RouterContract<MockBase>,
    relayer_nonce_suffix: &str,
) -> Result<Vec<Event>, CwEnvError> {
    let send_packets = extract_send_packet_events(&events);
    let packet = send_packets
        .into_iter()
        .next()
        .expect("expected at least one send_packet event");

    let relayer_address = router.query_relayer_addresses().unwrap().relayer_contract;
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    let call_data = euclid::msgs::router::ExecuteMsg::ReceivePacket {
        msg: packet.msg,
        sequence: packet.sequence,
        source_port: packet.source_port.clone(),
        destination_port: packet.destination_port.clone(),
        timeout: packet.timeout,
        encoding: 0,
    };
    let source_chain_uid = packet.source_port.split('.').next().unwrap();
    let signed_data = sign_relay_messsage(
        to_json_binary(&call_data).unwrap(),
        router.address().unwrap(),
        format!(
            "{}-{}-{}-receive-{}",
            packet.source_port, packet.destination_port, packet.sequence, relayer_nonce_suffix,
        ),
        &router.environment().app.borrow(),
        source_chain_uid,
    );

    let response = relayer.execute_meta_transaction(signed_data)?;
    Ok(response.events)
}

/// Mirror of `redeliver_factory_send_packet` for the router→factory direction:
/// re-delivers an ack-bound packet to the factory with a fresh relayer-level
/// nonce so the factory's contract-level state checks fire instead of the
/// relayer's meta-tx dedup.
pub fn redeliver_factory_ack_packet(
    factory: &FactoryContract<MockBase>,
    events: Vec<Event>,
    chain_uid: &ChainUid,
    relayer_nonce_suffix: &str,
) -> Result<Vec<Event>, CwEnvError> {
    let write_ack_packets = extract_ack_packet_events(&events);
    let destination_port = format!("{}.{}", chain_uid.as_str(), factory.address().unwrap());
    let packet = write_ack_packets
        .into_iter()
        .find(|p| p.destination_port == destination_port)
        .expect("expected ack packet for factory");

    let relayer_address = factory.get_state().unwrap().relayer_contract;
    let relayer = get_relayer(factory.environment(), &Addr::unchecked(relayer_address));

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
            "{}-{}-{}-ack-{}",
            packet.source_port, packet.destination_port, packet.sequence, relayer_nonce_suffix,
        ),
        &factory.environment().app.borrow(),
        source_chain_uid,
    );

    let response = relayer.execute_meta_transaction(signed_data)?;
    Ok(response.events)
}

pub fn relay_router_send_packet(
    events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let send_packets = extract_send_packet_events(&events);

    println!("Relay router send packets to factory:");
    println!("send packet count: {:?}", send_packets.len());

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
            timeout: packet.timeout,
            encoding: packet.encoding,
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
    println!("write ack packet count: {:?}", write_ack_packets.len());
    let relayer_address = factory.get_state().unwrap().relayer_contract;
    let relayer = get_relayer(factory.environment(), &Addr::unchecked(relayer_address));

    let destination_port = format!("{}.{}", chain_uid.as_str(), factory.address().unwrap());
    for packet in write_ack_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        println!("Ack packet: {:?}", packet.ack);

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
    _chain_uid: &ChainUid,
    events: Vec<Event>,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let write_ack_packets = extract_ack_packet_events(&events);
    println!("Relay router acknowledge packets:");
    println!("write ack packet count: {:?}", write_ack_packets.len());
    let relayer_address = router.query_relayer_addresses().unwrap().relayer_contract;
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    for packet in write_ack_packets {
        println!("Packet sequence: {:?}", packet.sequence);
        println!("Source port: {:?}", packet.source_port);
        println!("Destination port: {:?}", packet.destination_port);
        println!("Ack packet: {:?}", packet.ack);
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

    // The EVM leg rides canonical ABI, so the ack (and the echoed original msg)
    // must be ABI wire bytes; the router transcodes them back keyed by the send
    // tag. Encoding a JSON ack here would fail the router's strict ABI decode.
    let register_msg = FactoryReceiveMsg::RegisterFactory(RegisterFactorySendMsg {
        chain_uid: chain_uid.clone(),
        chain_type: RegisterFactoryChainType::Evm(RegisterFactoryChainEvm {
            factory_address: factory_address.to_string(),
            factory_chain_id: chain_id.to_string(),
        }),
        tx_id: tx_id.to_string(),
    });
    let wire_msg = register_msg.clone();
    let tag = wire_msg.wire_tag();
    let ack_wire = euclid_ibc::wire::transcode::factory_ack_json_to_wire(
        tag,
        to_json_binary(&ack).unwrap().as_slice(),
        euclid_encoding::Encoding::Abi,
    )
    .unwrap();
    let msg_wire = euclid_encoding::encode(&wire_msg, euclid_encoding::Encoding::Abi).unwrap();
    // Abi leg entry point fields ride the 0x lowercase hex transport form.
    let ack_string =
        euclid_encoding::repr::to_transport_string(&ack_wire, euclid_encoding::Encoding::Abi)
            .unwrap();
    let msg_string =
        euclid_encoding::repr::to_transport_string(&msg_wire, euclid_encoding::Encoding::Abi)
            .unwrap();

    let relayer_address = router.query_relayer_addresses().unwrap().relayer_contract;
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    let evm_port = format!("{}.{}", chain_uid.as_str(), factory_address).to_string();
    let vsl_port = format!("vsl.{}", router.address().unwrap()).to_string();

    let call_data = euclid::msgs::router::ExecuteMsg::AcknowledgePacket {
        source_port: evm_port.clone(),
        destination_port: vsl_port.clone(),
        msg: msg_string,
        sequence,
        ack: ack_string,
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

/// Required attribute value from an event by key.
fn get_event_attr<'a>(event: &'a Event, key: &str) -> &'a str {
    event
        .attributes
        .iter()
        .find(|attr| attr.key == key)
        .unwrap_or_else(|| panic!("missing '{}' attribute in {} event", key, event.ty))
        .value
        .as_str()
}

/// A parsed send-packet event.
///
/// `msg` holds the event's `msg` attribute verbatim, in the transport
/// representation for the leg encoding (raw JSON text when `encoding` is `0`,
/// `0x` lowercase hex when `1`). Re-injection forwards it unchanged; recover
/// bytes with `euclid_encoding::repr::from_transport_string` when needed.
pub struct SendPacketEvent {
    pub msg: String,
    pub sequence: u128,
    pub source_port: String,
    pub destination_port: String,
    pub timeout: u64,
    pub encoding: u8,
    pub version: String,
}

/// Recover the wire bytes carried by a send packet's transport `String`.
fn send_packet_wire_bytes(packet: &SendPacketEvent) -> (Vec<u8>, euclid_encoding::Encoding) {
    let encoding = euclid_encoding::Encoding::from_u8(packet.encoding).unwrap();
    let bytes = euclid_encoding::repr::from_transport_string(&packet.msg, encoding)
        .expect("send packet msg should be a valid transport string");
    (bytes, encoding)
}

/// Decode a send packet's `msg` into the domain factory message, honoring the
/// leg encoding recorded on the event (Json passthrough or Abi decode).
pub fn decode_factory_receive_msg(packet: &SendPacketEvent) -> FactoryReceiveMsg {
    let (bytes, encoding) = send_packet_wire_bytes(packet);
    euclid_ibc::wire::transcode::decode_factory_receive(&bytes, encoding).unwrap()
}

/// Decode a send packet's `msg` into the domain router message, honoring the
/// leg encoding recorded on the event (Json passthrough or Abi decode).
pub fn decode_router_receive_msg(
    packet: &SendPacketEvent,
) -> euclid_ibc::wire::envelope::router::RouterReceiveMsg {
    let (bytes, encoding) = send_packet_wire_bytes(packet);
    euclid_ibc::wire::transcode::decode_router_receive(&bytes, encoding).unwrap()
}
pub fn extract_send_packet_events(events: &[Event]) -> Vec<SendPacketEvent> {
    let mut send_packet_events = vec![];

    let send_packet_event_type = format!("wasm-{}", EUCLID_SEND_PACKET_ENCODED_EVENT);

    let related_events = events
        .iter()
        .filter(|event| event.ty == send_packet_event_type)
        .collect::<Vec<_>>();

    for event in related_events {
        let encoding = str::parse::<u8>(get_event_attr(event, "encoding")).unwrap();
        let msg = get_event_attr(event, "msg").to_string();
        let sequence = str::parse::<u128>(get_event_attr(event, "sequence")).unwrap();
        let source_port = get_event_attr(event, "source_port").to_string();
        let destination_port = get_event_attr(event, "destination_port").to_string();
        let timeout = str::parse::<u64>(get_event_attr(event, "timeout")).unwrap();
        let version = get_event_attr(event, "version").to_string();
        send_packet_events.push(SendPacketEvent {
            msg,
            sequence,
            source_port,
            destination_port,
            timeout,
            encoding,
            version,
        });
    }
    send_packet_events
}

/// A parsed ack-packet event.
///
/// `msg` and `ack` hold the attribute values verbatim, in the transport
/// representation for the leg encoding (see [`SendPacketEvent`]).
pub struct AckPacketEvent {
    pub msg: String,
    pub ack: String,
    pub sequence: u128,
    pub source_port: String,
    pub destination_port: String,
    pub encoding: u8,
    pub ack_type: String,
}
/// Extract ack-packet events from a list of wasm events.
///
/// Each `euclid-write-acknowledgement-encoded` event is complete and self
/// describing: it carries the ports (swapped, so `source_port` is the
/// acknowledging contract's port), the wire `msg` and `ack` (raw JSON text on
/// Json legs, 0x hex on Abi legs), `sequence`, `ack_type`, and `encoding`.
pub fn extract_ack_packet_events(events: &[Event]) -> Vec<AckPacketEvent> {
    let mut ack_packet_events = vec![];

    let ack_packet_event_type = format!("wasm-{}", EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT);

    let related_events = events
        .iter()
        .filter(|event| event.ty == ack_packet_event_type)
        .collect::<Vec<_>>();

    for event in related_events {
        let encoding = str::parse::<u8>(get_event_attr(event, "encoding")).unwrap();
        let msg = get_event_attr(event, "msg").to_string();
        let ack = get_event_attr(event, "ack").to_string();
        let sequence = str::parse::<u128>(get_event_attr(event, "sequence")).unwrap();
        let source_port = get_event_attr(event, "source_port").to_string();
        let destination_port = get_event_attr(event, "destination_port").to_string();
        let ack_type = get_event_attr(event, "ack_type").to_string();

        ack_packet_events.push(AckPacketEvent {
            msg,
            ack,
            sequence,
            source_port,
            destination_port,
            encoding,
            ack_type,
        });
    }
    ack_packet_events
}
