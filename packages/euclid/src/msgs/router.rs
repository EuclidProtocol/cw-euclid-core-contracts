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
    // Update Pool Code ID
    UpdateVLPCodeId { new_vlp_code_id: u64 },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StateResponse)]
    GetState {},
}
// We define a custom struct for each query response
#[cw_serde]
pub struct StateResponse {
    pub admin: String,
    pub vlp_code_id: u64,
}

#[cw_serde]
pub struct MigrateMsg {}
