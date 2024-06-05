use crate::token::{PairInfo, TokenInfo};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

use super::pool::{
    CallbackExecuteMsg, GetPairInfoResponse, GetPendingLiquidityResponse, GetPendingSwapsResponse,
    GetPoolReservesResponse, GetVLPResponse,
};
#[cw_serde]
pub struct InstantiateMsg {
    // Router contract on VLP
    pub router_contract: String,
    // Pool Code ID
    pub pool_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    // // Add Liquidity Request to the VLP
    // AddLiquidity {
    //     token_1_liquidity: Uint128,
    //     token_2_liquidity: Uint128,
    //     slippage_tolerance: u64,
    //     liquidity_id: String,
    //     timeout: Option<u64>,
    //     vlp_address: String,
    // },
    // ExecuteSwap {
    //     asset: Token,
    //     asset_amount: Uint128,
    //     min_amount_out: Uint128,
    //     swap_id: String,
    //     timeout: Option<u64>,
    //     vlp_address: String,
    // },
    // Request Pool Creation
    RequestPoolCreation {
        pair_info: PairInfo,
        timeout: Option<u64>,
    },
    // Update Pool Code ID
    UpdatePoolCodeId {
        new_pool_code_id: u64,
    },
    // Pool ExecuteMsgs //
    // Add Liquidity Request to the VLP
    AddLiquidityRequest {
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,
        slippage_tolerance: u64,
        timeout: Option<u64>,
    },
    ExecuteSwapRequest {
        asset: TokenInfo,
        asset_amount: Uint128,
        min_amount_out: Uint128,
        timeout: Option<u64>,
    },

    // Recieve CW20 TOKENS structure
    Receive(Cw20ReceiveMsg),
    Callback(CallbackExecuteMsg),
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

    // Pool Queries //
    #[returns(GetPairInfoResponse)]
    PairInfo {},
    #[returns(GetVLPResponse)]
    GetVlp {},
    // Fetch pending swaps with pagination for a user
    #[returns(GetPendingSwapsResponse)]
    PendingSwapsUser {
        user: String,
        lower_limit: Option<u128>,
        upper_limit: Option<u128>,
    },
    #[returns(GetPendingLiquidityResponse)]
    PendingLiquidity {
        user: String,
        lower_limit: Option<u128>,
        upper_limit: Option<u128>,
    },
    #[returns(GetPoolReservesResponse)]
    PoolReserves {},
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
    pub hub_channel: Option<String>,
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

#[cw_serde]
pub struct RegisterFactoryResponse {
    pub factory_address: String,
    pub chain_id: String,
}
