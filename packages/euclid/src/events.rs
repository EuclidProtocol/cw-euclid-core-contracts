use core::fmt;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Event, Uint128, Uint256};

use crate::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    deposit::DepositTokenRequest,
    swap::SwapRequest,
    token::{TokenMetadata, TokenType, TokenWithAmount},
};

pub fn liquidity_event(
    pool: &[TokenWithAmount],
    liquidity_change: &[TokenWithAmount],
    tx_id: &str,
) -> Event {
    let mut event = simple_event()
        .add_attribute("action", "liquidity_change")
        .add_attribute("tx_id", tx_id);

    for token in pool {
        event = event.add_attribute("token_id", token.token.to_string());
        event = event.add_attribute(format!("token_liquidity_{}", token.token), token.amount);
    }

    for token in liquidity_change {
        event = event.add_attribute(
            format!("token_liquidity_change_{}", token.token),
            token.amount,
        );
    }

    event
}

pub fn swap_event(tx_id: &str, swap: &SwapRequest) -> Event {
    simple_event()
        .add_attribute("action", "swap")
        .add_attribute("tx_id", tx_id)
        .add_attribute("asset_in", swap.asset_in.token.to_string())
        .add_attribute("asset_in_denom", swap.asset_in.token_type.get_key())
        .add_attribute("asset_out", swap.asset_out.to_string())
        .add_attribute("amount_in", swap.amount_in)
        .add_attribute("min_amount_out", swap.min_amount_out)
        .add_attribute("swaps", format!("{swaps:?}", swaps = swap.swaps))
}

pub fn deposit_token_event(tx_id: &str, deposit: &DepositTokenRequest) -> Event {
    simple_event()
        .add_attribute("action", "deposit_token")
        .add_attribute("tx_id", tx_id)
        .add_attribute("asset_in", deposit.asset_in.token.to_string())
        .add_attribute("asset_in_denom", deposit.asset_in.token_type.get_key())
        .add_attribute("amount_in", deposit.amount_in)
}

pub fn clp_add_liquidity_event(
    tx_id: &str,
    position_id: Uint128,
    liquidity_delta: Uint128,
    used_token_1: Uint128,
    used_token_2: Uint128,
) -> Event {
    simple_event()
        .add_attribute("action", "clp_add_liquidity")
        .add_attribute("tx_id", tx_id)
        .add_attribute("position_id", position_id)
        .add_attribute("liquidity_delta", liquidity_delta)
        .add_attribute("used_token_1", used_token_1)
        .add_attribute("used_token_2", used_token_2)
}

pub fn register_factory_event(
    tx_id: &str,
    factory_address: &str,
    channel: &str,
    router: &str,
) -> Event {
    simple_event()
        .add_attribute("action", "register_factory")
        .add_attribute("factory_address", factory_address)
        .add_attribute("channel", channel)
        .add_attribute("router", router)
        .add_attribute("tx_id", tx_id)
}

#[cw_serde]
pub enum TxType {
    Swap,
    DepositToken,
    AddLiquidity,
    RemoveLiquidity,
    PoolCreation,
    RegisterDenom,
    DeregisterDenom,
    EscrowRelease,
    TransferVoucher,
    EscrowWithdraw,
    RegisterFactory,
    UpdateFactoryChannel,
    WithdrawVirtualBalance,
    WithdrawVoucher,
    SingleSidedAddLiquidity,
}

impl fmt::Display for TxType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TxType::DepositToken => "deposit_token",
            TxType::Swap => "swap",
            TxType::AddLiquidity => "add_liquidity",
            TxType::RemoveLiquidity => "remove_liquidity",
            TxType::PoolCreation => "pool_creation",
            TxType::RegisterDenom => "register_denom",
            TxType::DeregisterDenom => "deregister_denom",
            TxType::EscrowRelease => "escrow_release",
            TxType::TransferVoucher => "transfer_voucher",
            TxType::EscrowWithdraw => "escrow_withdraw",
            TxType::RegisterFactory => "register_factory",
            TxType::UpdateFactoryChannel => "update_factory_channel",
            TxType::WithdrawVirtualBalance => "withdraw_virtual_balance",
            TxType::WithdrawVoucher => "withdraw_voucher",
            TxType::SingleSidedAddLiquidity => "single_sided_add_liquidity",
        };
        write!(f, "{}", s)
    }
}

pub fn tx_event(tx_id: &str, sender: &str, tx_type: TxType) -> Event {
    let tx_type = tx_type.to_string();
    simple_event()
        .add_attribute("action", "transaction")
        .add_attribute("tx_id", tx_id)
        .add_attribute("sender", sender)
        .add_attribute("type", tx_type)
}

pub fn simple_event() -> Event {
    Event::new("euclid").add_attribute("version", "1.0.0")
}

pub const EUCLID_RECEIVE_PACKET_EVENT: &str = "euclid-receive-packet";
pub const EUCLID_RECEIVE_ACKNOWLEDGEMENT_EVENT: &str = "euclid-receive-acknowledgement";

pub fn receive_packet_event(sequence: u128, source_port: &str, destination_port: &str) -> Event {
    Event::new(EUCLID_RECEIVE_PACKET_EVENT)
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("source_port", source_port)
        .add_attribute("destination_port", destination_port)
}

pub fn receive_acknowledgement_event(
    sequence: u128,
    source_port: &str,
    destination_port: &str,
) -> Event {
    Event::new(EUCLID_RECEIVE_ACKNOWLEDGEMENT_EVENT)
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("source_port", source_port)
        .add_attribute("destination_port", destination_port)
}

pub const EUCLID_TOKEN_METADATA_UPDATE_EVENT: &str = "euclid-token-metadata-update";
pub fn token_metadata_update_event(token_metadata: &TokenMetadata, action: &str) -> Event {
    Event::new(EUCLID_TOKEN_METADATA_UPDATE_EVENT)
        .add_attribute("action", action)
        .add_attribute("token", token_metadata.token.to_string())
        .add_attribute("chain_uid", token_metadata.chain_uid.to_string())
        .add_attribute("token_type", token_metadata.token_type.get_key())
        .add_attribute(
            "decimals",
            token_metadata
                .token_type
                .get_decimals()
                .unwrap_or_default()
                .to_string(),
        )
        .add_attribute("allowed", token_metadata.allowed.to_string())
}

pub const EUCLID_VIRTUAL_BALANCE_CHANGE_EVENT: &str = "euclid-virtual-balance-change";
pub fn virtual_balance_change_event(
    action: &str,
    amount: &Uint256,
    user: &CrossChainUser,
    token_id: &str,
) -> Event {
    Event::new(EUCLID_VIRTUAL_BALANCE_CHANGE_EVENT)
        .add_attribute("action", action)
        .add_attribute("amount", amount.to_string())
        .add_attribute("user", user.to_sender_string())
        .add_attribute("token_id", token_id)
}

pub const EUCLID_ESCROW_BALANCE_CHANGE_EVENT: &str = "euclid-escrow-balance-change";
pub fn escrow_balance_change_event(
    action: &str,
    amount: &Uint256,
    token_id: &str,
    chain_uid: &ChainUid,
    token_type: &TokenType,
) -> Event {
    Event::new(EUCLID_ESCROW_BALANCE_CHANGE_EVENT)
        .add_attribute("action", action)
        .add_attribute("amount", amount.to_string())
        .add_attribute("token_id", token_id)
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("token_type", token_type.get_key())
}

pub const EUCLID_FEE_OVERRIDE_CHANGE_EVENT: &str = "euclid-fee-override-change";
/// Emitted when the fee admin sets or removes a per-wallet Euclid-fee override.
///
/// `action` is `"set"` when an override is upserted and `"remove"` when an
/// entry is cleared. On set, `euclid_fee_bps` carries the new value; on remove
/// the attribute is omitted.
pub fn euclid_fee_override_change_event(
    action: &str,
    user: &CrossChainUser,
    euclid_fee_bps: Option<u64>,
) -> Event {
    let event = Event::new(EUCLID_FEE_OVERRIDE_CHANGE_EVENT)
        .add_attribute("action", action)
        .add_attribute("chain_uid", user.chain_uid.to_string())
        .add_attribute("address", user.address.clone());
    match euclid_fee_bps {
        Some(bps) => event.add_attribute("euclid_fee_bps", bps.to_string()),
        None => event,
    }
}

pub const EUCLID_SEND_PACKET_ENCODED_EVENT: &str = "euclid-send-packet-encoded";
pub const EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT: &str = "euclid-write-acknowledgement-encoded";

/// Attributes are emitted in the canonical cross-VM order, matching the
/// Solidity `SendPacket` event field order so a single indexer schema reads
/// both VMs.
#[allow(clippy::too_many_arguments)]
pub fn send_packet_encoded_event(
    msg: &str,
    sequence: u128,
    source_port: &str,
    destination_port: &str,
    timeout: u64,
    destination_chain_type: &str,
    version: &str,
    encoding: u8,
) -> Event {
    Event::new(EUCLID_SEND_PACKET_ENCODED_EVENT)
        .add_attribute("msg", msg)
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("source_port", source_port)
        .add_attribute("destination_port", destination_port)
        .add_attribute("timeout", timeout.to_string())
        .add_attribute("destination_chain_type", destination_chain_type)
        .add_attribute("version", version)
        .add_attribute("encoding", encoding.to_string())
}

/// The single complete acknowledgement event, emitted exactly once from the
/// cross-chain receive reply handler. Ports are emitted swapped versus the
/// incoming `ReceivePacket`: the acknowledging side is the event's
/// `source_port`. Attributes are emitted in the canonical cross-VM order,
/// matching the Solidity `WriteAcknowledgement` event field order.
#[allow(clippy::too_many_arguments)]
pub fn write_acknowledgement_encoded_event(
    msg: &str,
    sequence: u128,
    source_port: &str,
    destination_port: &str,
    ack: &str,
    destination_chain_type: &str,
    ack_type: &str,
    version: &str,
    encoding: u8,
) -> Event {
    Event::new(EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT)
        .add_attribute("msg", msg)
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("source_port", source_port)
        .add_attribute("destination_port", destination_port)
        .add_attribute("ack", ack)
        .add_attribute("destination_chain_type", destination_chain_type)
        .add_attribute("ack_type", ack_type)
        .add_attribute("version", version)
        .add_attribute("encoding", encoding.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_packet_encoded_event_attr_order() {
        let e = send_packet_encoded_event("{}", 7, "src", "dst", 99, "evm", "0.0.1", 1);
        assert_eq!(e.ty, EUCLID_SEND_PACKET_ENCODED_EVENT);
        let pairs: Vec<(&str, &str)> = e
            .attributes
            .iter()
            .map(|a| (a.key.as_str(), a.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("msg", "{}"),
                ("sequence", "7"),
                ("source_port", "src"),
                ("destination_port", "dst"),
                ("timeout", "99"),
                ("destination_chain_type", "evm"),
                ("version", "0.0.1"),
                ("encoding", "1"),
            ]
        );
    }

    #[test]
    fn write_acknowledgement_encoded_event_attr_order() {
        let e = write_acknowledgement_encoded_event(
            "{}", 7, "src", "dst", "AAAA", "cosmos", "error", "0.0.1", 0,
        );
        assert_eq!(e.ty, EUCLID_WRITE_ACKNOWLEDGEMENT_ENCODED_EVENT);
        let pairs: Vec<(&str, &str)> = e
            .attributes
            .iter()
            .map(|a| (a.key.as_str(), a.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("msg", "{}"),
                ("sequence", "7"),
                ("source_port", "src"),
                ("destination_port", "dst"),
                ("ack", "AAAA"),
                ("destination_chain_type", "cosmos"),
                ("ack_type", "error"),
                ("version", "0.0.1"),
                ("encoding", "0"),
            ]
        );
    }
}
