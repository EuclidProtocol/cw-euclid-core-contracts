use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Empty, QueryRequest, QueryResponse};

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {}

#[cw_serde]
#[derive(QueryResponses)]

pub enum QueryMsg {
    #[returns(MultiQueryResponse)]
    MultiQuery { queries: Vec<MultiQuery> },
}

#[cw_serde]
pub enum MultiQuery {
    Query(QueryRequest<Empty>),
    RawQuery(String),
}

// We define a custom struct for each query response
#[cw_serde]
pub struct MultiQueryResponse {
    pub responses: Vec<SingleQueryResponse>,
}

// We define a custom struct for each query response
#[cw_serde]
pub struct SingleQueryResponse {
    pub result: Option<QueryResponse>,
    pub err: Option<String>,
}
