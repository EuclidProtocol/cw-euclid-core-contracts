use cosmwasm_std::{ensure, Binary, Storage, Uint256};
use cw_storage_plus::Map;
use euclid::chain::Chain;
use euclid::chain::ChainUid;
use euclid::error::ContractError;
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

pub(crate) fn create_pending_packet_and_update_sequence(
    storage: &mut dyn Storage,
    chain: &Chain,
    msg: &Binary,
    ack_response: Option<Binary>,
    sender: &str,
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
) -> Result<(), ContractError> {
    let _existing_request =
        CROSS_CHAIN_PENDING_SEND_PACKETS.load(storage, (chain_uid.clone(), sequence))?;
    let _sender = CROSS_CHAIN_PENDING_PACKET_SENDER.load(storage, (chain_uid.clone(), sequence))?;

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

    Ok(())
}
