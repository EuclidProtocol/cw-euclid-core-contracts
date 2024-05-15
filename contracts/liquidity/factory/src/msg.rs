use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use euclid::{pool::Pool, token::{PairInfo, TokenInfo}};

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
        token_1_reserve: Uint128,
        token_2_reserve: Uint128,
        channel: String
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}


// Pool Instantitate Msg
#[cw_serde]
pub struct PoolInstantiateMsg {
    // VLP Contract Address
    pub vlp_contract: String,
    // Token Pair
    pub token_pair: PairInfo,
    // Pool Info
    pub pool: Pool,
    // Chain ID
    pub chain_id: String,
}