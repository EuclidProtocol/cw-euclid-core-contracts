use crate::{
    chain::{ChainUid, CrossChainUser},
    fee::{Fee, TotalFees},
    swap::NextSwapVlp,
    token::{Pair, Token},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub vcoin: String,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
    pub admin: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Registers a new pool from a new chain to an already existing VLP
    RegisterPool {
        sender: CrossChainUser,
        pair: Pair,
        tx_id: String,
    },

    // Registers a new pool from a new chain to an already existing VLP
    UpdateFee {
        lp_fee_bps: Option<u64>,
        euclid_fee_bps: Option<u64>,
        recipient: Option<CrossChainUser>,
    },

    Swap {
        sender: CrossChainUser,
        tx_id: String,
        asset_in: Token,
        amount_in: Uint128,
        min_token_out: Uint128,
        next_swaps: Vec<NextSwapVlp>,
        test_fail: Option<bool>,
    },
    AddLiquidity {
        sender: CrossChainUser,
        tx_id: String,
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
    },
    RemoveLiquidity {
        sender: CrossChainUser,
        tx_id: String,
        lp_allocation: Uint128,
    },
    /*

    // Update the fee for the VLP
    UpdateFee {
        lp_fee: u64,
        treasury_fee: u64,
        staker_fee: u64,
    },
    */
}

#[cw_serde]
#[derive(QueryResponses)]

pub enum QueryMsg {
    #[returns(GetStateResponse)]
    State {},
    // Query to simulate a swap for the asset
    #[returns(GetSwapResponse)]
    SimulateSwap {
        asset: Token,
        asset_amount: Uint128,
        swaps: Vec<NextSwapVlp>,
    },
    // Queries the total reserve of the pair in the VLP
    #[returns(GetLiquidityResponse)]
    Liquidity {},

    // Queries the fee of this specific pool
    #[returns(FeeResponse)]
    Fee {},

    #[returns(TotalFeesResponse)]
    TotalFeesCollected {},

    #[returns(TotalFeesPerDenomResponse)]
    TotalFeesPerDenom { denom: String },

    // Queries the pool information for a chain id
    #[returns(PoolResponse)]
    Pool { chain_uid: ChainUid },
    // Query to get all pools
    #[returns(AllPoolsResponse)]
    GetAllPools {},
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetSwapResponse {
    pub amount_out: Uint128,
    pub asset_out: Token,
}

#[cw_serde]
pub struct GetStateResponse {
    pub pair: Pair,
    pub router: String,
    pub vcoin: String,
    pub fee: Fee,
    pub last_updated: u64,
    pub total_lp_tokens: Uint128,
    pub admin: String,
}

#[cw_serde]
pub struct GetLiquidityResponse {
    pub pair: Pair,
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
    pub total_lp_tokens: Uint128,
}

#[cw_serde]
pub struct FeeResponse {
    pub fee: Fee,
}

#[cw_serde]
pub struct TotalFeesResponse {
    pub total_fees: TotalFees,
}

#[cw_serde]
pub struct TotalFeesPerDenomResponse {
    pub lp_fees: Uint128,
    pub euclid_fees: Uint128,
}

#[cw_serde]
pub struct PoolResponse {
    pub lp_shares: Uint128,
    pub reserve_1: Uint128,
    pub reserve_2: Uint128,
}

#[cw_serde]
pub struct PoolInfo {
    pub chain_uid: ChainUid,
    pub pool: PoolResponse,
}
#[cw_serde]
pub struct AllPoolsResponse {
    pub pools: Vec<PoolInfo>,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct VlpRemoveLiquidityResponse {
    pub token_1_liquidity_released: Uint128,
    pub token_2_liquidity_released: Uint128,
    pub burn_lp_tokens: Uint128,
    pub tx_id: String,
    pub sender: CrossChainUser,
    pub vlp_address: String,
}

#[cw_serde]
pub struct VlpSwapResponse {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub asset_out: Token,
    pub amount_out: Uint128,
}
