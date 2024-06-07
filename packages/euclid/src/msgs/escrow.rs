use crate::{
    pool::Pool,
    token::{PairInfo, Token, TokenInfo},
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

use super::pool::{
    GetPairInfoResponse, GetPendingLiquidityResponse, GetPendingSwapsResponse,
    GetPoolReservesResponse, GetVLPResponse,
};
#[cw_serde]
pub struct InstantiateMsg {
    // The only allowed Token ID for the contract
    pub token_id: Token,
    // Possibly add allowed denoms in Instantiation
}

#[cw_serde]
pub enum ExecuteMsg {
    // Updates allowed denoms
    UpdateAllowedDenoms { denoms: Vec<String> },

    // Recieve CW20 TOKENS structure
    Receive(Cw20ReceiveMsg),
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
    // pub pool_code_id: u64,
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
