use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Binary, Storage, Uint256};
use cw_storage_plus::{Item, Map};
use euclid::chain::Chain;
use euclid::chain::ChainUid;
use euclid::error::ContractError;
use euclid_encoding::EncodingError;
use euclid_ibc::state::PendingPacket;
use std::ops::Add;

// Cross Chain latest sequence count. Used to generate next sequence for send packet
pub const CROSS_CHAIN_LATEST_SEQUENCE_COUNT: Map<ChainUid, u128> =
    Map::new("cross_chain_latest_sequence_count");

// Cross Chain pending packets. Used to track total number of pending packets
pub const CROSS_CHAIN_PENDING_PACKETS_COUNT: Map<ChainUid, u128> =
    Map::new("cross_chain_pending_packets_count");

// Cross Chain Message original state, added during send packet, removed during ack packet
pub const CROSS_CHAIN_PENDING_SEND_PACKETS: Map<(ChainUid, u128), PendingPacket> =
    Map::new("cross_chain_pending_send_packets");

pub const CROSS_CHAIN_PENDING_PACKET_SENDER: Map<(ChainUid, u128), String> =
    Map::new("cross_chain_pending_packet_sender");

// Cross Chain processed sequence. Used to track the sequence of the processed packets
pub const CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS: Map<(ChainUid, u128), Uint256> =
    Map::new("cross_chain_processed_received_packets");

/// Context of the `ReceivePacket` currently being processed, consumed by the
/// cross-chain receive reply to transcode the ack onto the leg encoding and
/// emit the single complete acknowledgement event. A single `Item` suffices
/// because exactly one `ReceivePacket` is processed per transaction and the
/// reply fires in the same transaction.
///
/// `timeout` is deliberately not stored: the acknowledgement event schema
/// carries no timeout attribute and the timeout check happens inside the
/// internal callback, which receives it as a parameter.
#[cw_serde]
pub struct InFlightReceive {
    pub chain_uid: ChainUid,
    /// As received in `ReceivePacket`; the emit swaps the ports.
    pub source_port: String,
    /// As received in `ReceivePacket`; the emit swaps the ports.
    pub destination_port: String,
    /// Wire bytes as received, for the single shot event.
    pub msg: Binary,
    pub sequence: u128,
    /// Chain type of the incoming packet's source chain. The acknowledgement
    /// event emits it as `destination_chain_type` because the event swaps
    /// the ports (the ack travels back to that chain).
    pub source_chain_type: String,
    pub encoding: u8,
    pub wire_tag: u8,
}
pub const IN_FLIGHT_RECEIVE: Item<InFlightReceive> = Item::new("in_flight_receive");

/// Uniform mapping of codec errors onto the contract error type.
pub(crate) fn map_encoding_err(err: EncodingError) -> ContractError {
    ContractError::new(&err.to_string())
}

pub(crate) fn create_pending_packet_and_update_sequence(
    storage: &mut dyn Storage,
    chain: &Chain,
    msg: &Binary,
    ack_response: Option<Binary>,
    sender: &str,
    encoding: u8,
    wire_msg: Binary,
) -> Result<(ChainUid, u128), ContractError> {
    let chain_uid = chain.chain_uid.clone();
    let sequence = CROSS_CHAIN_LATEST_SEQUENCE_COUNT
        .load(storage, chain_uid.clone())
        .unwrap_or(0);
    let pending_packet_key = (chain_uid.clone(), sequence);

    // Make sure that the potential pending packet doesn't already exist
    ensure!(
        !CROSS_CHAIN_PENDING_SEND_PACKETS.has(storage, pending_packet_key.clone()),
        ContractError::Generic {
            err: "Pending packet already exists".to_string()
        }
    );

    CROSS_CHAIN_PENDING_SEND_PACKETS.save(
        storage,
        pending_packet_key.clone(),
        &PendingPacket {
            chain_uid: chain_uid.clone(),
            original_msg: msg.clone(),
            ack_response,
            encoding,
            wire_msg,
        },
    )?;
    CROSS_CHAIN_PENDING_PACKET_SENDER.save(storage, pending_packet_key, &sender.to_string())?;
    CROSS_CHAIN_LATEST_SEQUENCE_COUNT.save(storage, chain_uid.clone(), &sequence.add(1))?;

    let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
        .may_load(storage, chain_uid.clone())?
        .unwrap_or(0);

    CROSS_CHAIN_PENDING_PACKETS_COUNT.save(
        storage,
        chain_uid.clone(),
        &count.checked_add(1).ok_or(ContractError::new("Overflow"))?,
    )?;

    Ok((chain_uid, sequence))
}

pub(crate) fn remove_pending_packet_and_decrement_count(
    storage: &mut dyn Storage,
    chain_uid: &ChainUid,
    sequence: u128,
) -> Result<PendingPacket, ContractError> {
    let existing_request =
        CROSS_CHAIN_PENDING_SEND_PACKETS.load(storage, (chain_uid.clone(), sequence))?;

    // Remove the existing request as its already relayed now
    CROSS_CHAIN_PENDING_SEND_PACKETS.remove(storage, (chain_uid.clone(), sequence));
    CROSS_CHAIN_PENDING_PACKET_SENDER.remove(storage, (chain_uid.clone(), sequence));

    // Decrease the pending packets count as this packet is already processed
    let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
        .may_load(storage, chain_uid.clone())?
        // Getting to a point where the item wasn't loaded shouldn't be possible, but just in case, we're defaulting to 1 to avoid overflow error in the next operation
        .unwrap_or(1);

    CROSS_CHAIN_PENDING_PACKETS_COUNT.save(
        storage,
        chain_uid.clone(),
        &count.checked_sub(1).ok_or(ContractError::new("Overflow"))?,
    )?;

    Ok(existing_request)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{message_info, mock_dependencies};
    use cosmwasm_std::{to_json_binary, Binary};
    use euclid::chain::ChainType;

    use super::*;
    use crate::testing::helpers::{init, MockDeps};

    fn make_deps() -> MockDeps {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        init(deps.as_mut(), message_info(&creator, &[]));
        deps
    }

    /// A registered chain fixture. The router keys every relay map by
    /// `ChainUid`, so each test names the chain it is talking about.
    fn chain(uid: &str) -> Chain {
        Chain {
            chain_uid: ChainUid::create(uid.to_string()).unwrap(),
            factory_address: format!("factory_{uid}"),
            chain_type: ChainType::Native {},
        }
    }

    fn chain_uid(uid: &str) -> ChainUid {
        ChainUid::create(uid.to_string()).unwrap()
    }

    // -----------------------------------------------------------------------
    // create_pending_packet_and_update_sequence
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_pending_packet_first_call_returns_sequence_zero() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        let (uid, seq) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();

        assert_eq!(uid, chain_uid("chain1"));
        assert_eq!(seq, 0);
    }

    #[test]
    fn test_create_pending_packet_second_call_returns_sequence_one() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        let (_, seq) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();

        assert_eq!(seq, 1);
    }

    #[test]
    fn test_create_pending_packet_increments_cross_chain_pending_packets_count() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();

        let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage, chain_uid("chain1"))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_create_pending_packet_stores_packet_at_sequence() {
        let mut deps = make_deps();
        let msg = to_json_binary("my_message").unwrap();
        let ack = to_json_binary("ack_data").unwrap();
        let wire_msg = to_json_binary("wire_bytes").unwrap();

        let key = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            Some(ack.clone()),
            "alice",
            1,
            wire_msg.clone(),
        )
        .unwrap();

        let stored = CROSS_CHAIN_PENDING_SEND_PACKETS
            .load(&deps.storage, key)
            .unwrap();
        assert_eq!(stored.original_msg, msg);
        assert_eq!(stored.ack_response, Some(ack));
        // The router records the counterparty chain, not the VSL chain uid.
        assert_eq!(stored.chain_uid, chain_uid("chain1"));
        // The leg encoding is recorded for the ack path.
        assert_eq!(stored.encoding, 1);
        // The emitted wire bytes are stored verbatim for the ack byte check.
        assert_eq!(stored.wire_msg, wire_msg);
    }

    #[test]
    fn test_create_pending_packet_stores_wire_msg_verbatim() {
        let mut deps = make_deps();
        let msg = to_json_binary("my_message").unwrap();
        // Raw non-JSON bytes: the wire_msg is opaque to the relay state and must
        // survive the round trip byte for byte.
        let wire_msg = Binary::new(vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x01]);

        let key = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            2,
            wire_msg.clone(),
        )
        .unwrap();

        let stored = CROSS_CHAIN_PENDING_SEND_PACKETS
            .load(&deps.storage, key)
            .unwrap();
        assert_eq!(stored.wire_msg, wire_msg);
        assert_eq!(stored.wire_msg.as_slice(), &[0xde, 0xad, 0xbe, 0xef, 0, 1]);
    }

    #[test]
    fn test_create_pending_packet_stores_sender_at_sequence() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        let key = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();

        let stored_sender = CROSS_CHAIN_PENDING_PACKET_SENDER
            .load(&deps.storage, key)
            .unwrap();
        assert_eq!(stored_sender, "alice".to_string());
    }

    #[test]
    fn test_create_pending_packet_error_on_duplicate_sequence() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        // Manually place a packet at sequence 0 without advancing the counter so
        // the next call tries to write to the same slot.
        CROSS_CHAIN_PENDING_SEND_PACKETS
            .save(
                deps.as_mut().storage,
                (chain_uid("chain1"), 0u128),
                &PendingPacket {
                    chain_uid: chain_uid("chain1"),
                    original_msg: msg.clone(),
                    ack_response: None,
                    encoding: 0,
                    wire_msg: Binary::default(),
                },
            )
            .unwrap();

        let err = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap_err();

        assert_eq!(
            err,
            ContractError::Generic {
                err: "Pending packet already exists".to_string()
            }
        );
    }

    // -----------------------------------------------------------------------
    // Per-chain isolation (router specific: every relay map is keyed by ChainUid)
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_pending_packet_sequences_are_isolated_per_chain() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        // chain1 advances to sequence 1.
        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        let (_, chain1_second) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        assert_eq!(chain1_second, 1);

        // chain2 still starts at 0: the counters do not collide.
        let (uid, chain2_first) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain2"),
            &msg,
            None,
            "bob",
            0,
            Binary::default(),
        )
        .unwrap();
        assert_eq!(uid, chain_uid("chain2"));
        assert_eq!(chain2_first, 0);

        // Both chains hold a distinct packet at their own sequence 0.
        let chain1_packet = CROSS_CHAIN_PENDING_SEND_PACKETS
            .load(&deps.storage, (chain_uid("chain1"), 0u128))
            .unwrap();
        let chain2_packet = CROSS_CHAIN_PENDING_SEND_PACKETS
            .load(&deps.storage, (chain_uid("chain2"), 0u128))
            .unwrap();
        assert_eq!(chain1_packet.chain_uid, chain_uid("chain1"));
        assert_eq!(chain2_packet.chain_uid, chain_uid("chain2"));

        assert_eq!(
            CROSS_CHAIN_LATEST_SEQUENCE_COUNT
                .load(&deps.storage, chain_uid("chain1"))
                .unwrap(),
            2
        );
        assert_eq!(
            CROSS_CHAIN_LATEST_SEQUENCE_COUNT
                .load(&deps.storage, chain_uid("chain2"))
                .unwrap(),
            1
        );
    }

    #[test]
    fn test_create_pending_packet_counts_are_isolated_per_chain() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain2"),
            &msg,
            None,
            "bob",
            0,
            Binary::default(),
        )
        .unwrap();

        assert_eq!(
            CROSS_CHAIN_PENDING_PACKETS_COUNT
                .load(&deps.storage, chain_uid("chain1"))
                .unwrap(),
            2
        );
        assert_eq!(
            CROSS_CHAIN_PENDING_PACKETS_COUNT
                .load(&deps.storage, chain_uid("chain2"))
                .unwrap(),
            1
        );
    }

    // -----------------------------------------------------------------------
    // remove_pending_packet_and_decrement_count
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_pending_packet_returns_saved_packet() {
        let mut deps = make_deps();
        let msg = to_json_binary("hello").unwrap();
        let wire_msg = to_json_binary("wire").unwrap();

        let (uid, seq) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            3,
            wire_msg.clone(),
        )
        .unwrap();

        let packet =
            remove_pending_packet_and_decrement_count(deps.as_mut().storage, &uid, seq).unwrap();

        assert_eq!(packet.original_msg, msg);
        assert_eq!(packet.chain_uid, chain_uid("chain1"));
        assert_eq!(packet.encoding, 3);
        assert_eq!(packet.wire_msg, wire_msg);
    }

    #[test]
    fn test_remove_pending_packet_removes_packet_entry() {
        let mut deps = make_deps();
        let msg = to_json_binary("hello").unwrap();

        let (uid, seq) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        remove_pending_packet_and_decrement_count(deps.as_mut().storage, &uid, seq).unwrap();

        assert!(!CROSS_CHAIN_PENDING_SEND_PACKETS.has(&deps.storage, (uid, seq)));
    }

    #[test]
    fn test_remove_pending_packet_removes_sender_entry() {
        let mut deps = make_deps();
        let msg = to_json_binary("hello").unwrap();

        let (uid, seq) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        remove_pending_packet_and_decrement_count(deps.as_mut().storage, &uid, seq).unwrap();

        assert!(!CROSS_CHAIN_PENDING_PACKET_SENDER.has(&deps.storage, (uid, seq)));
    }

    #[test]
    fn test_remove_pending_packet_decrements_cross_chain_pending_packets_count() {
        let mut deps = make_deps();
        let msg = to_json_binary("hello").unwrap();

        let (uid, seq) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();

        let before = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage, chain_uid("chain1"))
            .unwrap();
        assert_eq!(before, 1);

        remove_pending_packet_and_decrement_count(deps.as_mut().storage, &uid, seq).unwrap();

        let after = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage, chain_uid("chain1"))
            .unwrap();
        assert_eq!(after, 0);
    }

    #[test]
    fn test_remove_pending_packet_error_on_missing_sequence() {
        let mut deps = make_deps();

        let err = remove_pending_packet_and_decrement_count(
            deps.as_mut().storage,
            &chain_uid("chain1"),
            99u128,
        )
        .unwrap_err();

        // The load will produce a StdError wrapped in ContractError; ensure it is an error.
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn test_remove_pending_packet_error_on_wrong_chain() {
        let mut deps = make_deps();
        let msg = to_json_binary("hello").unwrap();

        let (_, seq) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();

        // Same sequence, different chain: the (chain_uid, sequence) key does not exist.
        let err = remove_pending_packet_and_decrement_count(
            deps.as_mut().storage,
            &chain_uid("chain2"),
            seq,
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Std(_)));

        // chain1's packet is untouched.
        assert!(CROSS_CHAIN_PENDING_SEND_PACKETS.has(&deps.storage, (chain_uid("chain1"), seq)));
    }

    #[test]
    fn test_round_trip_create_then_remove_restores_count_to_zero() {
        let mut deps = make_deps();
        let msg = to_json_binary("payload").unwrap();

        let (uid, seq0) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();
        let (_, seq1) = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &chain("chain1"),
            &msg,
            None,
            "alice",
            0,
            Binary::default(),
        )
        .unwrap();

        remove_pending_packet_and_decrement_count(deps.as_mut().storage, &uid, seq0).unwrap();
        remove_pending_packet_and_decrement_count(deps.as_mut().storage, &uid, seq1).unwrap();

        let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage, chain_uid("chain1"))
            .unwrap();
        assert_eq!(count, 0);

        // The sequence counter is monotonic: it is not rewound on remove.
        let latest = CROSS_CHAIN_LATEST_SEQUENCE_COUNT
            .load(&deps.storage, chain_uid("chain1"))
            .unwrap();
        assert_eq!(latest, 2);
    }
}
