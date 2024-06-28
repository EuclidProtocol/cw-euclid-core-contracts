use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use euclid::{
    swap::NextSwap,
    token::{Pair, Token},
};

// Message that implements an ExecuteSwap on the VLP contract

#[cw_serde]
pub enum ChainIbcExecuteMsg {
    // Request Pool Creation
    RequestPoolCreation {
        pair: Pair,
        // Factory will set this using info.sender
        sender: String,
        // Unique per tx
        tx_id: String,
    },
    AddLiquidity {
        // Factory will set this using info.sender
        sender: String,

        // User will provide this data and factory will verify using info funds
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,

        // User will provide this data
        slippage_tolerance: u64,

        vlp_address: String,

        // Unique per tx
        tx_id: String,
    },

    // Remove liquidity from a chain pool to VLP
    RemoveLiquidity {
        // Factory will set this using info.sender
        sender: String,

        lp_allocation: Uint128,
        vlp_address: String,

        // Unique per tx
        tx_id: String,
    },

    // Swap tokens on VLP
    Swap(ChainIbcSwapExecuteMsg),
    // New Factory Msg
    RequestWithdraw {
        token_id: Token,
        amount: Uint128,

        // Factory will set this using info.sender
        sender: String,

        // User will provide this data
        to_address: String,
        to_chain_uid: String,

        // Unique per tx
        tx_id: String,
    },
    RequestEscrowCreation {
        token_id: Token,
        // Factory will set this using info.sender
        sender: String,
        // Unique per tx
        tx_id: String,
        //TODO Add allowed denoms?
    },
}

#[cw_serde]
pub struct ChainIbcSwapExecuteMsg {
    // Factory will set this to info.sender
    pub sender: String,
    // Factory will set this to info.sender
    pub to_address: String,
    pub to_chain_uid: String,

    // User will provide this
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwap>,

    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub enum HubIbcExecuteMsg {
    // Send Factory Registration Message from Router to Factory
    RegisterFactory {
        router: String,
        // Unique per tx
        tx_id: String,
    },

    ReleaseEscrow {
        amount: Uint128,
        token_id: String,
        to_address: String,
        to_chain_uid: String,

        // Unique per tx
        tx_id: String,
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
