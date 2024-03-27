use cosmwasm_schema::{cw_serde, QueryResponses};
use euclid::{fee::Fee, pool::Pool, token::Pair};

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
        pool: Pool
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
