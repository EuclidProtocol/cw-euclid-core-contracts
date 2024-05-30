use crate::token::{PairInfo, Token};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
#[cw_serde]
pub struct InstantiateMsg {
    // Pool Code ID
    pub vlp_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Request Pool Creation
    RequestPoolCreation {
        pair_info: PairInfo,
        channel: String,
    },
    ExecuteSwap {
        asset: Token,
        asset_amount: Uint128,
        min_amount_out: Uint128,
        channel: String,
        swap_id: String,
    },
    // Add Liquidity Request to the VLP
    AddLiquidity {
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        channel: String,
        liquidity_id: String,
    },
    // Update Pool Code ID
    UpdatePoolCodeId {
        new_pool_code_id: u64,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(GetPoolResponse)]
    GetPool { vlp: String },
    #[returns(StateResponse)]
    GetState {},
    // Query to get all pools in the factory
    #[returns(AllPoolsResponse)]
    GetAllPools {},
}

#[cw_serde]
pub struct GetPoolResponse {
    pub pool: String,
}
// We define a custom struct for each query response
#[cw_serde]
pub struct StateResponse {
    pub chain_id: String,
    pub router_contract: String,
    pub admin: String,
    pub pool_code_id: u64,
}

#[cw_serde]
pub struct AllPoolsResponse {
    pub pools: Vec<PoolVlpResponse>, // Assuming pool addresses are strings
}
#[cw_serde]
pub struct PoolVlpResponse {
    pub pool: String,
    pub vlp: String,
}

#[cw_serde]
pub struct MigrateMsg {}
