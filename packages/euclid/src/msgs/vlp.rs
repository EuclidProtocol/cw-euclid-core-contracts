use crate::{
    fee::Fee,
    pool::Pool,
    swap::NextSwap,
    token::{Pair, PairInfo, Token},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub vcoin: String,
    pub cw20: String,
    pub pair: Pair,
    pub fee: Fee,
    pub execute: Option<ExecuteMsg>,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Registers a new pool from a new chain to an already existing VLP
    RegisterPool {
        chain_id: String,
        pair_info: PairInfo,
    },

    Swap {
        to_chain_id: String,
        to_address: String,
        asset_in: Token,
        amount_in: Uint128,
        min_token_out: Uint128,
        swap_id: String,
        next_swaps: Vec<NextSwap>,
    },
    AddLiquidity {
        chain_id: String,
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
    },
    RemoveLiquidity {
        chain_id: String,
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
    // Query to simulate a swap for the asset
    #[returns(GetSwapResponse)]
    SimulateSwap {
        asset: Token,
        asset_amount: Uint128,
        swaps: Vec<NextSwap>,
    },
    // Queries the total reserve of the pair in the VLP
    #[returns(GetLiquidityResponse)]
    Liquidity {},

    // Queries the fee of this specific pool
    #[returns(FeeResponse)]
    Fee {},

    // Queries the pool information for a chain id
    #[returns(PoolResponse)]
    Pool { chain_id: String },
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
pub struct GetLiquidityResponse {
    pub pair: Pair,
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
    pub total_lp_tokens: Uint128,
}

#[cw_serde]
pub struct PoolInfo {
    pub chain: String,
    pub pool: Pool,
}
#[cw_serde]
pub struct FeeResponse {
    pub fee: Fee,
}

#[cw_serde]
pub struct PoolResponse {
    pub pool: Pool,
}

#[cw_serde]
pub struct AllPoolsResponse {
    pub pools: Vec<PoolInfo>,
}

#[cw_serde]
pub struct MigrateMsg {}
