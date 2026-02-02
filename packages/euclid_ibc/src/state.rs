use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary};
use cw_storage_plus::{Item, Map};
use euclid::chain::ChainUid;

#[cw_serde]
pub struct PendingPacket {
    pub chain_uid: ChainUid,
    pub original_msg: Binary,
    pub ack_response: Option<Binary>,
}
// Store the original message that was sent
pub const NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE: Map<u64, PendingPacket> =
    Map::new("native_cross_chain_original_msg_reply_queue");

pub const NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER: Map<u64, Addr> =
    Map::new("native_cross_chain_pending_packet_sender");

// Store the current count of the queue
pub const NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT: Item<u64> =
    Item::new("native_cross_chain_original_msg_reply_queue_count");

// Range of reply IDS reserved for native cross chain messages
pub const NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE: (u64, u64) = (2001, 3000);
