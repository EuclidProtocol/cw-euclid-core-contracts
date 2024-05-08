use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use euclid::{fee::Fee, pool::Pool, token::{Pair, Token}};

#[cw_serde]
pub struct InstantiateMsg {
    pub router: String,
    pub pair: Pair,
    pub fee: Fee,
    pub pool: Pool,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Registers a new pool from a new chain to an already existing VLP
    RegisterPool {
        pool: Pool,
    },

    AddLiquidity {
        chain_id: String,
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        channel: String,
  
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
    },
    // Queries the total reserve of the pair in the VLP
    #[returns(GetLiquidityResponse)]
    Liquidity {},

    // Queries the total reserve of the pair with info in the VLP
    #[returns(LiquidityInfoResponse)]
    LiquidityInfo {},
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetSwapResponse {
    pub token_out: Uint128,
}

#[cw_serde]
pub struct GetLiquidityResponse {
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
}


#[cw_serde]
pub struct LiquidityInfoResponse {
    pub pair: Pair,
    pub token_1_reserve: Uint128,
    pub token_2_reserve: Uint128,
}
