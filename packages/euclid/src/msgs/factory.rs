use crate::token::{PairInfo, Token};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
#[cw_serde]
pub struct InstantiateMsg {
    // Router contract on VLP
    pub router_contract: String,
    // Chain ID
    pub chain_id: String,
    // Pool Code ID
    pub pool_code_id: u64,
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
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(GetPoolResponse)]
    GetPool { vlp: String },
    #[returns(StateResponse)]
    GetState {},

    // Query connection counts for a given channel_id
    #[returns(ConnectionCountResponse)]
    GetConnectionCount { channel_id: String },

    // Query timeout counts for a given channel_id
    #[returns(TimeoutCountResponse)]
    GetTimeoutCount { channel_id: String },

    // Query the pool address for a given VLP address
    #[returns(PoolAddressResponse)]
    GetPoolAddress { vlp_address: String },
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
pub struct ConnectionCountResponse {
    pub count: u32,
}

#[cw_serde]
pub struct TimeoutCountResponse {
    pub count: u32,
}

#[cw_serde]
pub struct PoolAddressResponse {
    pub pool_address: String,
}
