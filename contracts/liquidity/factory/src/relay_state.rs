use cosmwasm_std::{ensure, Addr, Binary, Storage, Uint256};
use cw_storage_plus::{Item, Map};
use euclid::{chain::ChainUid, error::ContractError};
use euclid_ibc::state::PendingPacket;

use crate::rate_limit::{USER_PENDING_PACKETS_COUNT, USER_TOTAL_PACKETS_COUNT};

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
pub const CROSS_CHAIN_PROCESSED_RECEIVED_PACKETS: Map<u128, Uint256> =
    Map::new("cross_chain_processed_received_packets");

pub(crate) fn create_pending_packet_and_update_sequence(
    storage: &mut dyn Storage,
    msg: &Binary,
    ack_response: Option<Binary>,
    sender: &Addr,
) -> Result<u128, ContractError> {
    let sequence = CROSS_CHAIN_LATEST_SEQUENCE_COUNT.load(storage).unwrap_or(0);

    // Make sure the sequence is not already used, this is just an extra check to avoid duplicate sequence which can cause issues in receive packet event
    ensure!(
        !CROSS_CHAIN_PENDING_SEND_PACKETS.has(storage, sequence),
        ContractError::Generic {
            err: "Sequence already exists".to_string()
        }
    );

    CROSS_CHAIN_PENDING_SEND_PACKETS.save(
        storage,
        sequence,
        &PendingPacket {
            chain_uid: ChainUid::vsl_chain_uid()?,
            original_msg: msg.clone(),
            ack_response,
        },
    )?;
    CROSS_CHAIN_PENDING_PACKET_SENDER.save(storage, sequence, sender)?;
    CROSS_CHAIN_LATEST_SEQUENCE_COUNT.save(storage, &(sequence + 1))?;

    let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
        .may_load(storage)?
        .unwrap_or(0);
    CROSS_CHAIN_PENDING_PACKETS_COUNT.save(
        storage,
        &count.checked_add(1).ok_or(ContractError::new("Overflow"))?,
    )?;

    let user_pending_packets_count = USER_PENDING_PACKETS_COUNT
        .load(storage, sender.clone())
        .unwrap_or(0);

    // Update user pending packets count
    USER_PENDING_PACKETS_COUNT.save(
        storage,
        sender.clone(),
        &user_pending_packets_count
            .checked_add(1)
            .ok_or(ContractError::new("Overflow"))?,
    )?;
    USER_TOTAL_PACKETS_COUNT.update(
        storage,
        sender.clone(),
        |count| -> Result<_, ContractError> {
            count
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(ContractError::new("Overflow"))
        },
    )?;

    Ok(sequence)
}

pub(crate) fn remove_pending_packet_and_decrement_count(
    storage: &mut dyn Storage,
    sequence: u128,
) -> Result<(PendingPacket, Addr), ContractError> {
    let existing_request = CROSS_CHAIN_PENDING_SEND_PACKETS.load(storage, sequence)?;
    let sender = CROSS_CHAIN_PENDING_PACKET_SENDER.load(storage, sequence)?;

    // Remove the existing request as its already relayed now
    CROSS_CHAIN_PENDING_SEND_PACKETS.remove(storage, sequence);
    CROSS_CHAIN_PENDING_PACKET_SENDER.remove(storage, sequence);

    USER_PENDING_PACKETS_COUNT.update(
        storage,
        sender.clone(),
        |count| -> Result<_, ContractError> {
            count
                .unwrap_or(0)
                .checked_sub(1)
                .ok_or(ContractError::new("Overflow"))
        },
    )?;

    let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
        .may_load(storage)?
        // Getting to a point where the item wasn't loaded shouldn't be possible, but just in case, we're defaulting to 1 to avoid overflow error in the next operation
        .unwrap_or(1);

    CROSS_CHAIN_PENDING_PACKETS_COUNT.save(
        storage,
        &count.checked_sub(1).ok_or(ContractError::new("Overflow"))?,
    )?;

    Ok((existing_request, sender))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{testing::mock_dependencies, to_json_binary};

    use super::*;
    use crate::testing::helpers::{init, MockDeps};

    fn make_deps() -> MockDeps {
        let mut deps = mock_dependencies();
        init(&mut deps);
        deps
    }

    // -----------------------------------------------------------------------
    // create_pending_packet_and_update_sequence
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_pending_packet_first_call_returns_sequence_zero() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();

        assert_eq!(seq, 0);
    }

    #[test]
    fn test_create_pending_packet_second_call_returns_sequence_one() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
            .unwrap();
        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();

        assert_eq!(seq, 1);
    }

    #[test]
    fn test_create_pending_packet_increments_cross_chain_pending_packets_count() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
            .unwrap();
        create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
            .unwrap();

        let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage)
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_create_pending_packet_increments_user_pending_packets_count() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
            .unwrap();
        create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
            .unwrap();

        let count = crate::rate_limit::USER_PENDING_PACKETS_COUNT
            .load(&deps.storage, sender.clone())
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_create_pending_packet_increments_user_total_packets_count() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
            .unwrap();
        create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
            .unwrap();

        let count = crate::rate_limit::USER_TOTAL_PACKETS_COUNT
            .load(&deps.storage, sender.clone())
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_create_pending_packet_stores_packet_at_sequence() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("my_message").unwrap();
        let ack = to_json_binary("ack_data").unwrap();

        let seq = create_pending_packet_and_update_sequence(
            deps.as_mut().storage,
            &msg,
            Some(ack.clone()),
            &sender,
        )
        .unwrap();

        let stored = CROSS_CHAIN_PENDING_SEND_PACKETS
            .load(&deps.storage, seq)
            .unwrap();
        assert_eq!(stored.original_msg, msg);
        assert_eq!(stored.ack_response, Some(ack));
        assert_eq!(stored.chain_uid, ChainUid::vsl_chain_uid().unwrap());
    }

    #[test]
    fn test_create_pending_packet_stores_sender_at_sequence() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();

        let stored_sender = CROSS_CHAIN_PENDING_PACKET_SENDER
            .load(&deps.storage, seq)
            .unwrap();
        assert_eq!(stored_sender, sender);
    }

    #[test]
    fn test_create_pending_packet_error_on_duplicate_sequence() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        // Manually place a packet at sequence 0 without advancing the counter so
        // the next call tries to write to the same slot.
        CROSS_CHAIN_PENDING_SEND_PACKETS
            .save(
                deps.as_mut().storage,
                0u128,
                &PendingPacket {
                    chain_uid: ChainUid::vsl_chain_uid().unwrap(),
                    original_msg: msg.clone(),
                    ack_response: None,
                },
            )
            .unwrap();

        let err =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap_err();

        assert_eq!(
            err,
            ContractError::Generic {
                err: "Sequence already exists".to_string()
            }
        );
    }

    // -----------------------------------------------------------------------
    // remove_pending_packet_and_decrement_count
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_pending_packet_returns_saved_packet_and_sender() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("hello").unwrap();

        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();

        let (packet, returned_sender) =
            remove_pending_packet_and_decrement_count(deps.as_mut().storage, seq).unwrap();

        assert_eq!(packet.original_msg, msg);
        assert_eq!(returned_sender, sender);
    }

    #[test]
    fn test_remove_pending_packet_removes_packet_entry() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("hello").unwrap();

        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();
        remove_pending_packet_and_decrement_count(deps.as_mut().storage, seq).unwrap();

        assert!(!CROSS_CHAIN_PENDING_SEND_PACKETS.has(&deps.storage, seq));
    }

    #[test]
    fn test_remove_pending_packet_removes_sender_entry() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("hello").unwrap();

        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();
        remove_pending_packet_and_decrement_count(deps.as_mut().storage, seq).unwrap();

        assert!(!CROSS_CHAIN_PENDING_PACKET_SENDER.has(&deps.storage, seq));
    }

    #[test]
    fn test_remove_pending_packet_decrements_cross_chain_pending_packets_count() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("hello").unwrap();

        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();

        let before = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage)
            .unwrap();
        assert_eq!(before, 1);

        remove_pending_packet_and_decrement_count(deps.as_mut().storage, seq).unwrap();

        let after = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage)
            .unwrap();
        assert_eq!(after, 0);
    }

    #[test]
    fn test_remove_pending_packet_decrements_user_pending_packets_count() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("hello").unwrap();

        let seq =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();

        remove_pending_packet_and_decrement_count(deps.as_mut().storage, seq).unwrap();

        let count = crate::rate_limit::USER_PENDING_PACKETS_COUNT
            .load(&deps.storage, sender.clone())
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_remove_pending_packet_error_on_missing_sequence() {
        let mut deps = make_deps();

        let err =
            remove_pending_packet_and_decrement_count(deps.as_mut().storage, 99u128).unwrap_err();

        // The load will produce a StdError wrapped in ContractError; ensure it is an error.
        assert!(matches!(err, ContractError::Std(_)));
    }

    #[test]
    fn test_round_trip_create_then_remove_restores_count_to_zero() {
        let mut deps = make_deps();
        let sender = deps.api.addr_make("alice");
        let msg = to_json_binary("payload").unwrap();

        let seq0 =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();
        let seq1 =
            create_pending_packet_and_update_sequence(deps.as_mut().storage, &msg, None, &sender)
                .unwrap();

        remove_pending_packet_and_decrement_count(deps.as_mut().storage, seq0).unwrap();
        remove_pending_packet_and_decrement_count(deps.as_mut().storage, seq1).unwrap();

        let count = CROSS_CHAIN_PENDING_PACKETS_COUNT
            .load(&deps.storage)
            .unwrap();
        assert_eq!(count, 0);

        let user_count = crate::rate_limit::USER_PENDING_PACKETS_COUNT
            .load(&deps.storage, sender.clone())
            .unwrap();
        assert_eq!(user_count, 0);

        // USER_TOTAL_PACKETS_COUNT is not decremented on remove — it tracks lifetime
        // total, not current pending.
        let total = crate::rate_limit::USER_TOTAL_PACKETS_COUNT
            .load(&deps.storage, sender)
            .unwrap();
        assert_eq!(total, 2);
    }
}
