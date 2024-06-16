use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use euclid::{
    swap::NextSwap,
    token::{PairInfo, Token},
};

// Message that implements an ExecuteSwap on the VLP contract

#[cw_serde]
pub enum ChainIbcExecuteMsg {
    // Request Pool Creation
    RequestPoolCreation {
        pool_rq_id: String,
        pair_info: PairInfo,
    },
    AddLiquidity {
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        liquidity_id: String,
        pool_address: String,
        vlp_address: String,
    },

    // Remove liquidity from a chain pool to VLP
    RemoveLiquidity {
        chain_id: String,
        lp_allocation: Uint128,
        vlp_address: String,
    },

    // Swap tokens on VLP
    Swap {
        to_address: String,
        to_chain_id: String,
        asset_in: Token,
        amount_in: Uint128,
        min_amount_out: Uint128,
        swap_id: String,
        swaps: Vec<NextSwap>,
    },
    // New Factory Msg
    RequestWithdraw {
        token_id: Token,
        recipient: String,
        amount: Uint128,
        chain_id: String,
    },
    RequestEscrowCreation {
        token_id: Token,
        //TODO Add allowed denoms?
    },
}

#[cw_serde]
pub enum HubIbcExecuteMsg {
    // Send Factory Registration Message from Router to Factory
    RegisterFactory { router: String },
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
