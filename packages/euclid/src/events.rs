use core::fmt;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::Event;

use crate::{pool::Pool, swap::SwapRequest};

pub fn liquidity_event(pool: &Pool, tx_id: &str) -> Event {
    Event::new("euclid")
        .add_attribute("constant", "euclid")
        .add_attribute("action", "liquidity_change")
        .add_attribute("token_1_id", pool.pair.token_1.to_string())
        .add_attribute("token_1_liquidity", pool.reserve_1)
        .add_attribute("token_2_id", pool.pair.token_2.to_string())
        .add_attribute("token_2_liquidity", pool.reserve_2)
        .add_attribute("tx_id", tx_id)
}

pub fn swap_event(tx_id: &str, swap: &SwapRequest) -> Event {
    Event::new("euclid")
        .add_attribute("constant", "euclid")
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

pub fn register_factory_event(
    tx_id: &str,
    factory_address: &str,
    channel: &str,
    router: &str,
) -> Event {
    Event::new("euclid")
        .add_attribute("constant", "euclid")
        .add_attribute("action", "register_factory")
        .add_attribute("factory_address", factory_address)
        .add_attribute("channel", channel)
        .add_attribute("router", router)
        .add_attribute("tx_id", tx_id)
}

#[cw_serde]
pub enum TxType {
    Swap,
    AddLiquidity,
    RemoveLiquidity,
    PoolCreation,
    EscrowRelease,
    EscrowWithdraw,
    RegisterFactory,
}

impl fmt::Display for TxType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TxType::Swap => "swap",
            TxType::AddLiquidity => "add_liquidity",
            TxType::RemoveLiquidity => "remove_liquidity",
            TxType::PoolCreation => "pool_creation",
            TxType::EscrowRelease => "escrow_release",
            TxType::EscrowWithdraw => "escrow_withdraw",
            TxType::RegisterFactory => "register_factory",
        };
        write!(f, "{}", s)
    }
}

pub fn tx_event(tx_id: &str, sender: &str, tx_type: TxType) -> Event {
    let tx_type = match tx_type {
        TxType::AddLiquidity => "add_liquidity".to_string(),
        t => format!("{t:?}"),
    };
    Event::new("euclid")
        .add_attribute("action", "transaction")
        .add_attribute("tx_id", tx_id)
        .add_attribute("sender", sender)
        .add_attribute("type", tx_type)
}
