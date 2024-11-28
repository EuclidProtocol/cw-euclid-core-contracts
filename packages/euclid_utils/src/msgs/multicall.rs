use cosmwasm_schema::QueryResponses;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Empty, QueryRequest, QueryResponse};

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct InstantiateMsg {}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum ExecuteMsg {}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[derive(QueryResponses)]

pub enum QueryMsg {
    #[returns(MultiQueryResponse)]
    MultiQuery { queries: Vec<MultiQuery> },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub enum MultiQuery {
    Query(QueryRequest<Empty>),
    RawQuery(String),
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct MultiQueryResponse {
    pub responses: Vec<SingleQueryResponse>,
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
pub struct SingleQueryResponse {
    pub result: Option<QueryResponse>,
    pub err: Option<String>,
}
