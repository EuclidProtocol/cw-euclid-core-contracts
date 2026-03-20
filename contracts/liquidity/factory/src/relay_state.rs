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
