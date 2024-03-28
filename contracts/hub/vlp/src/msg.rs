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
    // Add liquidity from a chain pool to VLP 
    AddLiquidity { 
        chain_id: String,
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
     },
     // Remove liquidity from a chain pool to VLP
    RemoveLiquidity {
        chain_id: String,
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        },
    // Swap tokens on VLP
    Swap {
        chain_id: String,
        asset: Token,
        asset_amount: Uint128,
        min_amount_out: Uint128,
        },
    // Update the fee for the VLP
    UpdateFee {
        lp_fee: u64,
        treasury_fee: u64,
        staker_fee: u64,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // GetCount returns the current count as a json-encoded number
    #[returns(GetCountResponse)]
    GetCount {},
}

// We define a custom struct for each query response
#[cw_serde]
pub struct GetCountResponse {
    pub count: i32,
}
