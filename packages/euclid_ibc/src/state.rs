use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary};
use cw_storage_plus::{Item, Map};
use euclid::chain::ChainUid;

#[cw_serde]
pub struct PendingPacket {
    pub chain_uid: ChainUid,
    pub original_msg: Binary, // wire enum JSON (byte identical to the old domain JSON)
    pub ack_response: Option<Binary>,
    #[serde(default)] // in flight packets from before the upgrade read as 0 = Json
    pub encoding: u8, // Encoding::as_u8 of the leg
    #[serde(default)] // Amendment B; empty for pre-upgrade packets, which then
    pub wire_msg: Binary, // fail the byte check (drain before upgrade, risk 2)
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

#[cfg(test)]
mod tests {
    use super::*;

    // In flight packets stored before the upgrade have no `encoding` key.
    // serde(default) must read them as 0 = Json.
    #[test]
    fn pending_packet_encoding_defaults_to_json() {
        let legacy_json = r#"{"chain_uid":"chain1","original_msg":"e30=","ack_response":null}"#;
        let packet: PendingPacket = cosmwasm_std::from_json(legacy_json.as_bytes()).unwrap();
        assert_eq!(packet.encoding, 0);
    }

    // Pre-amendment JSON packets (no `wire_msg` key) must deserialize with an
    // empty `wire_msg`; they then fail the Amendment B byte check as intended.
    #[test]
    fn pending_packet_wire_msg_defaults_to_empty() {
        let legacy_json = r#"{"chain_uid":"chain1","original_msg":"e30=","ack_response":null}"#;
        let packet: PendingPacket = cosmwasm_std::from_json(legacy_json.as_bytes()).unwrap();
        assert_eq!(packet.wire_msg, Binary::default());
    }
}
