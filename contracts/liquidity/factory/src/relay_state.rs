use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use euclid_ibc::state::PendingPacket;

// Cross Chain latest sequence count. Used to generate next sequence for send packet
pub const CROSS_CHAIN_LATEST_SEQUENCE_COUNT: Item<u128> =
    Item::new("cross_chain_latest_sequence_count");

// Cross Chain pending packets. Used to track total number of pending packets
pub const CROSS_CHAIN_PENDING_PACKETS_COUNT: Item<u128> =
    Item::new("cross_chain_pending_packets_count");

// Cross Chain Message original state, added during send packet, removed during ack packet
pub const CROSS_CHAIN_PENDING_SEND_PACKETS: Map<u128, PendingPacket> =
    Map::new("cross_chain_pending_send_packets");
pub const CROSS_CHAIN_PENDING_PACKET_SENDER: Map<u128, Addr> =
    Map::new("cross_chain_pending_packet_sender");

// Cross Chain processed sequence. Used to track the sequence of the processed packets
pub const CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS: Map<u128, Uint128> =
    Map::new("cross_chain_processed_received_packets");
