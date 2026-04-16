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

// Reply IDs 2001–3000 are reserved for native cross-chain messages (1 000 concurrent slots).
// The counter advances by 1 per use and wraps back to 2001 once it exceeds 3000.
// Wrap-around is best-effort reuse: a slot may still be occupied after a full cycle.
// The `ensure!` guard in the allocation path is the hard safety check — callers that
// exhaust all 1 000 in-flight slots simultaneously will receive a "Reply ID is already
// in use" / "Msg Queue is full" error until earlier replies are processed and slots freed.
pub const NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE: (u64, u64) = (2001, 3000);
