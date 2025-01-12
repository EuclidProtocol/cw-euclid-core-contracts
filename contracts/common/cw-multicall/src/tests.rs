use cosmwasm_std::{
    from_json,
    testing::{mock_dependencies, mock_dependencies_with_balances, mock_env, mock_info},
    BalanceResponse, BankQuery, Coin, DepsMut, Env, MessageInfo,
};
use euclid_utils::msgs::multicall::{InstantiateMsg, MultiQuery, MultiQueryResponse, QueryMsg};

use crate::contract::{instantiate, query};

fn init_cw_multicall(deps: DepsMut, env: Env, info: MessageInfo) {
    let msg = InstantiateMsg {};
    let res = instantiate(deps, env, info, msg);
    assert!(res.is_ok())
}

#[test]
fn test_instantiation() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let info = mock_info("creator", &[]);
    init_cw_multicall(deps.as_mut(), env, info)
}

#[test]
fn test_multiquery_call() {
    let coin = Coin::new(10000, "test");
    let info = mock_info("creator", &[]);
    let mut deps = mock_dependencies_with_balances(&[("creator", &[coin.clone()])]);
    let env = mock_env();

    init_cw_multicall(deps.as_mut(), env.clone(), info);

    let mut queries: Vec<MultiQuery> = vec![];
    let bank_query = BankQuery::Balance {
        address: "creator".to_string(),
        denom: coin.denom.clone(),
    };

    queries.push(MultiQuery::Query(bank_query.into()));
    // Add a raw query for the same bank query
    let raw_query = format!(
        "{{\"bank\":{{\"balance\":{{\"address\":\"{}\",\"denom\":\"{}\"}}}}}}",
        "creator", coin.denom
    );
    queries.push(MultiQuery::RawQuery(raw_query));

    let msg = QueryMsg::MultiQuery {
        queries: queries.clone(),
    };
    let result = query(deps.as_ref(), env.clone(), msg.clone()).unwrap();
    let result: MultiQueryResponse = from_json(result).unwrap();

    assert_eq!(
        result.responses.len(),
        queries.len(),
        "Queries == Responses"
    );

    let bank_response = result.responses.first().unwrap().clone();

    assert_eq!(bank_response.err, None, "Successful Query");

    assert_eq!(
        from_json::<BalanceResponse>(&bank_response.result.unwrap()).unwrap(),
        BalanceResponse {
            amount: coin.clone()
        },
        "Balance is same as assignined coin balance"
    );

    // Check the second query response (raw bank query)
    let raw_bank_response = result.responses.get(1).unwrap().clone();

    assert_eq!(raw_bank_response.err, None, "Successful Raw Query");

    let raw_balance_response: BalanceResponse =
        from_json(raw_bank_response.result.unwrap()).unwrap();
    assert_eq!(
        raw_balance_response,
        BalanceResponse { amount: coin },
        "Raw query balance is same as assigned coin balance"
    );
}
