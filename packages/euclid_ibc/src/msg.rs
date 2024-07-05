use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use euclid::{
    chain::{ChainUid, CrossChainUser},
    swap::NextSwapPair,
    token::{Pair, Token},
};

// Message that implements an ExecuteSwap on the VLP contract

#[cw_serde]
pub enum ChainIbcExecuteMsg {
    // Request Pool Creation
    RequestPoolCreation {
        // Factory will set this using info.sender
        sender: CrossChainUser,
        tx_id: String,

        pair: Pair,
    },
    AddLiquidity {
        // Factory will set this using info.sender
        sender: CrossChainUser,

        // User will provide this data and factory will verify using info funds
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,

        // User will provide this data
        slippage_tolerance: u64,

        pair: Pair,

        // Unique per tx
        tx_id: String,
    },

    // Remove liquidity from a chain pool to VLP
    RemoveLiquidity(ChainIbcRemoveLiquidityExecuteMsg),

    // Swap tokens on VLP
    Swap(ChainIbcSwapExecuteMsg),
    // RequestWithdraw {
    //     token_id: Token,
    //     amount: Uint128,

    //     // Factory will set this using info.sender
    //     sender: String,

    //     // First element in array has highest priority
    //     cross_chain_addresses: Vec<CrossChainUser>,

    //     // Unique per tx
    //     tx_id: String,
    // },
    // RequestEscrowCreation {
    //     token: Token,
    //     // Factory will set this using info.sender
    //     sender: String,
    //     // Unique per tx
    //     tx_id: String,
    //     //TODO Add allowed denoms?
    // },
}

#[cw_serde]
pub struct ChainIbcRemoveLiquidityExecuteMsg {
    // Factory will set this using info.sender
    pub sender: CrossChainUser,

    pub lp_allocation: Uint128,
    pub pair: Pair,

    // First element in array has highest priority
    pub cross_chain_addresses: Vec<CrossChainUser>,

    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct ChainIbcSwapExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,

    // User will provide this
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwapPair>,

    // First element in array has highest priority
    pub cross_chain_addresses: Vec<CrossChainUser>,

    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub enum HubIbcExecuteMsg {
    // Send Factory Registration Message from Router to Factory
    RegisterFactory {
        chain_uid: ChainUid,
        // Unique per tx
        tx_id: String,
    },

    ReleaseEscrow {
        sender: CrossChainUser,
        amount: Uint128,
        token: Token,
        to_address: String,

        // Unique per tx
        tx_id: String,
    },
}
