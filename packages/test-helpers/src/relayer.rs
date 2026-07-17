use std::str::FromStr;

use cosmwasm_std::{to_json_binary, to_json_string, Addr, Binary, Event};
use cw_orch::{
    core::CwEnvError,
    mock::{cw_multi_test::App, MockBase},
    prelude::{ContractInstance, Environment},
};
use euclid::{
    chain::ChainUid,
    events::{EUCLID_SEND_PACKET_ENCODED_EVENT, EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT},
    msgs::{factory::QueryMsgFns as FactoryQueryFns, router::QueryMsgFns as RouterQueryFns},
};
use factory::FactoryContract;
use k256::{ecdsa::SigningKey, elliptic_curve::NonZeroScalar};
use relayer::{
    ExecuteMsgFns as RelayerExecuteFns, MetaTransaction as RelayerMetaTransaction,
    MetaTransactionData as RelayerMetaTransactionData, ValidatorSignature,
};
use router::RouterContract;
use sha2::{digest::Update, Digest, Sha256};

use crate::chains::get_relayer;

/// Extract a required attribute value from an event by key.
/// Panics with a descriptive message if the attribute is missing.
fn get_event_attr<'a>(event: &'a Event, key: &str) -> &'a str {
    event
        .attributes
        .iter()
        .find(|attr| attr.key == key)
        .unwrap_or_else(|| panic!("missing '{}' attribute in {} event", key, event.ty))
        .value
        .as_str()
}

/// Relay factory→router send packets.
pub fn relay_factory_send_packet(
    events: Vec<Event>,
    router: &RouterContract<MockBase>,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let send_packets = extract_send_packet_events(&events);

    let relayer_address = router
        .query_relayer_addresses()
        .expect("router should have relayer addresses")
        .relayer_contract;
    let relayer = get_relayer(router.environment(), &Addr::unchecked(relayer_address));

    for packet in send_packets {
        let call_data = euclid::msgs::router::ExecuteMsg::ReceivePacket {
            msg: packet.msg,
            sequence: packet.sequence,
            source_port: packet.source_port.clone(),
            destination_port: packet.destination_port.clone(),
            timeout: packet.timeout,
            encoding: packet.encoding,
        };
        let source_chain_uid = packet
            .source_port
            .split('.')
            .next()
            .expect("source_port should contain '.' separator");
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).expect("call_data should serialize"),
            router.address().expect("router should have address"),
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

/// Relay factory ack packets back to the factory.
pub fn relay_factory_ack_packet(
    factory: &FactoryContract<MockBase>,
    events: Vec<Event>,
    chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let mut responses = Vec::new();

    let write_ack_packets = extract_ack_packet_events(&events);
    let relayer_address = factory
        .get_state()
        .expect("factory state should exist")
        .relayer_contract;
    let relayer = get_relayer(factory.environment(), &Addr::unchecked(relayer_address));

    let destination_port = format!(
        "{}.{}",
        chain_uid.as_str(),
        factory.address().expect("factory should have address")
    );
    for packet in write_ack_packets {
        if packet.destination_port != destination_port {
            continue;
        }

        let call_data = euclid::msgs::factory::ExecuteMsg::AcknowledgePacket {
            source_port: packet.source_port.clone(),
            destination_port: packet.destination_port.clone(),
            msg: packet.msg,
            sequence: packet.sequence,
            ack: packet.ack,
        };

        let source_chain_uid = packet
            .source_port
            .split('.')
            .next()
            .expect("source_port should contain '.' separator");
        let signed_data = sign_relay_messsage(
            to_json_binary(&call_data).expect("call_data should serialize"),
            factory.address().expect("factory should have address"),
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

/// Full factory→router→factory relay round-trip.
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

/// Get the hardcoded test signer key pair.
pub fn get_signer_key() -> (SigningKey, Binary) {
    let pk = "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369";
    get_signer_key_from_pk(pk)
}

/// Derive a Cosmos signer key pair from a hex-encoded private key.
pub fn get_signer_key_from_pk(pk: &str) -> (SigningKey, Binary) {
    let scalar = NonZeroScalar::from_str(pk).expect("invalid hex private key");

    let signer_key = SigningKey::from(scalar);
    let pubkey = signer_key
        .verifying_key()
        .to_encoded_point(true) // true = compressed format (33 bytes) for Cosmos
        .as_bytes()
        .to_vec();

    let pubkey_binary = Binary::from(pubkey);
    (signer_key, pubkey_binary)
}

/// Sign a relay meta-transaction with the test validator key.
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
    let msg = to_json_string(&meta_tx_data).expect("meta_tx_data should serialize");
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
        .expect("signing should succeed")
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
        chain_uid: ChainUid::create(source_chain_uid.to_string())
            .expect("source_chain_uid should be valid"),
    }
}

/// A parsed send-packet event from wasm event attributes.
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

/// Extract send-packet events from a list of wasm events.
pub fn extract_send_packet_events(events: &[Event]) -> Vec<SendPacketEvent> {
    let mut send_packet_events = vec![];

    let send_packet_event_type = format!("wasm-{}", EUCLID_SEND_PACKET_ENCODED_EVENT);

    let related_events = events
        .iter()
        .filter(|event| event.ty == send_packet_event_type)
        .collect::<Vec<_>>();

    for event in related_events {
        let encoding: u8 = get_event_attr(event, "encoding")
            .parse()
            .expect("encoding attribute should be a valid u8");
        let msg = get_event_attr(event, "msg").to_string();
        let sequence: u128 = get_event_attr(event, "sequence")
            .parse()
            .expect("sequence attribute should be a valid u128");
        let source_port = get_event_attr(event, "source_port").to_string();
        let destination_port = get_event_attr(event, "destination_port").to_string();
        let timeout: u64 = get_event_attr(event, "timeout")
            .parse()
            .expect("timeout attribute should be a valid u64");
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

/// A parsed ack-packet event from wasm event attributes.
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
/// Json legs, `0x` prefixed lowercase hex on Abi legs), `sequence`,
/// `ack_type`, and `encoding`.
pub fn extract_ack_packet_events(events: &[Event]) -> Vec<AckPacketEvent> {
    let mut ack_packet_events = vec![];

    let ack_packet_event_type = format!("wasm-{}", EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT);

    let related_events = events
        .iter()
        .filter(|event| event.ty == ack_packet_event_type)
        .collect::<Vec<_>>();

    for event in related_events {
        let encoding: u8 = get_event_attr(event, "encoding")
            .parse()
            .expect("encoding attribute should be a valid u8");
        let msg = get_event_attr(event, "msg").to_string();
        let ack = get_event_attr(event, "ack").to_string();
        let sequence: u128 = get_event_attr(event, "sequence")
            .parse()
            .expect("sequence attribute should be a valid u128");
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
