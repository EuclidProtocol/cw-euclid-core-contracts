use std::str::FromStr;

use cosmwasm_std::{from_json, to_json_binary, to_json_string, Addr, Binary, Event};
use cw_orch::{
    core::CwEnvError,
    mock::{cw_multi_test::App, MockBase},
    prelude::{ContractInstance, Environment},
};
use euclid::{
    chain::ChainUid,
    msgs::{factory::QueryMsgFns, router::QueryMsgFns as RouterQueryFns},
};
use euclid_ibc::msg::ChainIbcExecuteMsg;
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

        let sequence = str::parse::<u128>(sequence.value.as_str()).unwrap();
        println!("relay_router_send_packet: {:?}", sequence);
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
        println!("relay_factory_ack_packet: {:?}", msg_enum);
        println!("relay_factory_ack_packet: {:?}", sequence);

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

        let response = relayer.execute_meta_transaction(signed_data)?;

        responses.extend(response.events);
    }
    Ok(responses)
}

pub fn relay_factory_router_factory(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    factory_chain_uid: &ChainUid,
) -> Result<Vec<Event>, CwEnvError> {
    let ack_events = relay_factory_send_packet(send_events, router, factory_chain_uid)?;
    relay_factory_ack_packet(factory, factory_chain_uid, ack_events.clone())?;
    Ok(ack_events)
}

#[allow(dead_code)]
pub fn relay_router_factory_router(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
    router: &RouterContract<MockBase>,
) -> Result<(), CwEnvError> {
    let ack_events = relay_router_send_packet(send_events, factory, factory_chain_uid)?;
    relay_router_ack_packet(router, factory_chain_uid, ack_events)?;
    Ok(())
}

pub fn get_signer_key() -> SigningKey {
    let pk = "2268A9118C1681EC6A649F01886995DE55E90C7E71B0BC5E409C551B92FF7369";
    let scalar = NonZeroScalar::from_str(pk).unwrap();
    SigningKey::from(scalar)
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

    let secret_key = get_signer_key();
    let signature = secret_key
        .sign_digest_recoverable(message_digest)
        .unwrap()
        .0;
    MetaTransaction {
        data: msg,
        signature: Binary::from(signature.to_vec()),
    }
}
