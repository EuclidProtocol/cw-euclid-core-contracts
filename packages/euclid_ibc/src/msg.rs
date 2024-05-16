use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use euclid::token::{PairInfo, Token};

// Message that implements an ExecuteSwap on the VLP contract

#[cw_serde]
pub enum IbcExecuteMsg {
    AddLiquidity {
        chain_id: String,
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        liquidity_id: String,
    },

    // Remove liquidity from a chain pool to VLP
    RemoveLiquidity {
        chain_id: String,
        lp_allocation: Uint128,
    },

    // Swap tokens on VLP
    Swap {
        chain_id: String,
        asset: Token,
        asset_amount: Uint128,
        min_amount_out: Uint128,
        channel: String,
        swap_id: String,
    },

    // Request Pool Creation
    RequestPoolCreation {
        chain_id: String,
        pair_info: PairInfo,
        token_1_reserve: Uint128,
        token_2_reserve: Uint128,
        pool_rq_id: String,
    },
}

/// A custom acknowledgement type.
/// The success type `T` depends on the PacketMsg variant.
///
/// This could be refactored to use [StdAck] at some point. However,
/// it has a different success variant name ("ok" vs. "result") and
/// a JSON payload instead of a binary payload.
///
/// [StdAck]: https://github.com/CosmWasm/cosmwasm/issues/1512
#[cw_serde]
pub enum AcknowledgementMsg<S> {
    Ok(S),
    Error(String),
}

impl<S> AcknowledgementMsg<S> {
    pub fn unwrap(self) -> S {
        match self {
            AcknowledgementMsg::Ok(data) => data,
            AcknowledgementMsg::Error(err) => panic!("{}", err),
        }
    }

    pub fn unwrap_err(self) -> String {
        match self {
            AcknowledgementMsg::Ok(_) => panic!("not an error"),
            AcknowledgementMsg::Error(err) => err,
        }
    }
}
