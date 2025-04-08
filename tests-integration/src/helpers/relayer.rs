use cosmwasm_std::{from_json, Binary, Event};
use cw_orch::mock::MockBase;
use euclid::{
    chain::ChainUid,
    msgs::{
        factory::ExecuteMsgFns as FactoryExecuteFns, router::ExecuteMsgFns as RouterExecuteFns,
    },
};
use euclid_ibc::msg::ChainIbcExecuteMsg;
use factory::FactoryContract;
use router::RouterContract;

pub fn relay_factory_send_packet(
    events: Vec<Event>,
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
) -> Vec<Event> {
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

        let response = router
            .cosmos_receive_packet(chain_uid.clone(), hash.value.clone(), msg_binary, sequence)
            .unwrap();

        responses.extend(response.events);
    }

    responses
}

pub fn relay_router_send_packet(
    events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
) -> Vec<Event> {
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

        let response = factory
            .cosmos_receive_packet(hash.value.clone(), msg_binary, sequence)
            .unwrap();

        responses.extend(response.events);
    }
    responses
}

pub fn relay_factory_ack_packet(
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
    events: Vec<Event>,
) -> Vec<Event> {
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

        let response = factory
            .cosmos_receive_ack(ack_binary, hash.value.clone(), msg_binary, sequence)
            .unwrap();

        responses.extend(response.events);
    }
    responses
}

pub fn relay_router_ack_packet(
    router: &RouterContract<MockBase>,
    chain_uid: &ChainUid,
    events: Vec<Event>,
) -> Vec<Event> {
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

        let response = router
            .cosmos_receive_ack(
                ack_binary,
                chain_uid.clone(),
                hash.value.clone(),
                msg_binary,
                sequence,
            )
            .unwrap();

        responses.extend(response.events);
    }
    responses
}

pub fn relay_factory_router_factory(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    router: &RouterContract<MockBase>,
    factory_chain_uid: &ChainUid,
) {
    let ack_events = relay_factory_send_packet(send_events, router, factory_chain_uid);
    relay_factory_ack_packet(factory, factory_chain_uid, ack_events);
}

pub fn relay_router_factory_router(
    send_events: Vec<Event>,
    factory: &FactoryContract<MockBase>,
    factory_chain_uid: &ChainUid,
    router: &RouterContract<MockBase>,
) {
    let ack_events = relay_router_send_packet(send_events, factory, factory_chain_uid);
    relay_router_ack_packet(router, factory_chain_uid, ack_events);
}
