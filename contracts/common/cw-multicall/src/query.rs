use cosmwasm_std::{to_json_binary, to_json_vec, Binary, Deps, QueryResponse};
use euclid::error::ContractError;
use euclid_utils::msgs::multicall::{MultiQuery, MultiQueryResponse, SingleQueryResponse};

/*
/// Executes multiple queries in a single call.
///
/// This function takes a vector of `MultiQuery` objects and executes each query,
/// collecting the results into a `MultiQueryResponse`. If a query fails, the error
/// is captured in the corresponding `SingleQueryResponse`.
///
/// # Arguments
///
/// * `deps` - The dependencies object providing access to storage, API, and Querier.
/// * `queries` - A vector of `MultiQuery` objects representing the queries to be executed.
///
/// # Returns
///
/// Returns a `Result` containing a `Binary` representation of `MultiQueryResponse` on success,
/// or a `ContractError` on failure.

*/
pub fn query_multi_queries(deps: Deps, queries: Vec<MultiQuery>) -> Result<Binary, ContractError> {
    let responses = queries
        .iter()
        .map(|query| {
            let res = query_multi_query(deps, query).map_err(|err| err.to_string());
            SingleQueryResponse {
                // We don't want to throw error otherwise the whole multicall will fail
                result: res.clone().ok(),
                err: res.err(),
            }
        })
        .collect();
    Ok(to_json_binary(&MultiQueryResponse { responses })?)
}

/*
Executes a single query from a `MultiQuery` object.

This function takes a `MultiQuery` object and executes the query it contains,
returning the raw query response.

# Arguments

* `deps` - The dependencies object providing access to storage, API, and Querier.
* `query` - A reference to a `MultiQuery` object representing the query to be executed.

# Returns

Returns a `Result` containing a `QueryResponse` on success,
or a `ContractError` on failure.
*/
fn query_multi_query(deps: Deps, query: &MultiQuery) -> Result<QueryResponse, ContractError> {
    // As we don't know the response type, we will use raw query to get binary response
    let raw_query = match query {
        MultiQuery::Query(query) => to_json_vec(&query)?,
        MultiQuery::RawQuery(query) => query.as_bytes().to_vec(),
    };
    let result = deps
        .querier
        .raw_query(&raw_query)
        .into_result()
        .map_err(|err| ContractError::new(&err.to_string()))?;

    result.into_result().map_err(|err| ContractError::new(&err))
}
