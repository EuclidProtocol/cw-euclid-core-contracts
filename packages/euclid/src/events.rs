use core::fmt;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Event;

use crate::{
    deposit::DepositTokenRequest,
    swap::SwapRequest,
    token::{Token, TokenType, TokenWithAmount},
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

pub fn register_denom_event(token: &Token, chain_uid: &str, denom: &TokenType) -> Event {
    Event::new("euclid-register-denom")
        .add_attribute("token", token.to_string())
        .add_attribute(format!("{}_chain_uid", token), chain_uid)
        .add_attribute(format!("{}_denom", token), denom.get_key())
}

pub fn deregister_denom_event(token: &Token, chain_uid: &str, denom: &TokenType) -> Event {
    Event::new("euclid-deregister-denom")
        .add_attribute("token", token.to_string())
        .add_attribute(format!("{}_chain_uid", token), chain_uid)
        .add_attribute(format!("{}_denom", token), denom.get_key())
}

pub const EUCLID_SEND_PACKET_EVENT: &str = "euclid-send-packet";
pub const EUCLID_RECEIVE_PACKET_EVENT: &str = "euclid-receive-packet";
pub const EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT: &str = "euclid-write-acknowledgement";
pub const EUCLID_RECEIVE_ACKNOWLEDGEMENT_EVENT: &str = "euclid-receive-acknowledgement";

pub fn send_packet_event(
    source_port: &str,
    destination_port: &str,
    msg: &str,
    sequence: u128,
    timeout: u64,
    destination_chain_type: &str,
) -> Event {
    Event::new(EUCLID_SEND_PACKET_EVENT)
        .add_attribute("source_port", source_port)
        .add_attribute("destination_port", destination_port)
        .add_attribute("msg", msg)
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("timeout", timeout.to_string())
        .add_attribute("destination_chain_type", destination_chain_type)
}

pub fn receive_packet_event(sequence: u128, source_port: &str, destination_port: &str) -> Event {
    Event::new(EUCLID_RECEIVE_PACKET_EVENT)
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("source_port", source_port)
        .add_attribute("destination_port", destination_port)
}

// Write acknowledgement event is triggered by the contract itself after receiving a packet. This will also have ack msg but its not present at the time this event is released and hence will be added later.
pub fn write_acknowledgement_event(
    sequence: u128,
    source_port: &str,
    destination_port: &str,
    destination_chain_type: &str,
    msg: &str,
) -> Event {
    Event::new(EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT)
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("source_port", source_port)
        .add_attribute("destination_port", destination_port)
        .add_attribute("destination_chain_type", destination_chain_type)
        .add_attribute("msg", msg)
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
