use core::fmt;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Event;

use crate::{deposit::DepositTokenRequest, swap::SwapRequest, token::TokenWithAmount};

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
        .add_attribute("timeout", format!("{timeout:?}", timeout = swap.timeout))
}

pub fn deposit_token_event(tx_id: &str, deposit: &DepositTokenRequest) -> Event {
    simple_event()
        .add_attribute("action", "deposit_token")
        .add_attribute("tx_id", tx_id)
        .add_attribute("asset_in", deposit.asset_in.token.to_string())
        .add_attribute("asset_in_denom", deposit.asset_in.token_type.get_key())
        .add_attribute("amount_in", deposit.amount_in)
        .add_attribute("timeout", format!("{timeout:?}", timeout = deposit.timeout))
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
    EscrowCreation,
    EscrowRelease,
    TransferVirtualBalance,
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
            TxType::EscrowCreation => "escrow_creation",
            TxType::EscrowRelease => "escrow_release",
            TxType::TransferVirtualBalance => "transfer_virtual_balance",
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
