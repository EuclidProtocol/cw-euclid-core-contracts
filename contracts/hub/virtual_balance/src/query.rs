use cosmwasm_std::{to_json_binary, Binary, Deps, Uint128};
use euclid::{
    chain::ChainUid,
    error::ContractError,
    generate_query_all,
    msgs::virtual_balance::{
        AllAllowancesResponse, AllBalancesResponse, GetBalanceResponse, GetStateResponse,
        GetUserBalancesResponse, GetUserBalancesResponseItem, VBalanceMigrateMsg,
    },
    virtual_balance::BalanceKey,
};

use crate::state::{ALLOWANCES, BALANCES, STATE};

pub fn query_state(deps: Deps) -> Result<GetStateResponse, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(GetStateResponse { state })
}

pub fn query_balance(deps: Deps, balance_key: BalanceKey) -> Result<Binary, ContractError> {
    let balance = BALANCES.may_load(
        deps.storage,
        balance_key.clone().to_serialized_balance_key(),
    )?;
    Ok(to_json_binary(&GetBalanceResponse {
        amount: balance.unwrap_or(Uint128::zero()),
    })?)
}

pub fn query_user_balances(
    deps: Deps,
    chain_uid: ChainUid,
    address: String,
) -> Result<Binary, ContractError> {
    let balances: Result<_, ContractError> = BALANCES
        .prefix((chain_uid.clone(), address.clone()))
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|res| {
            let res = res?;
            Ok(GetUserBalancesResponseItem {
                token_id: res.0,
                amount: res.1,
            })
        })
        .collect();

    Ok(to_json_binary(&GetUserBalancesResponse {
        balances: balances?,
    })?)
}

generate_query_all!(query_balances, BALANCES, AllBalancesResponse, balances);

generate_query_all!(
    query_allowances,
    ALLOWANCES,
    AllAllowancesResponse,
    allowances
);

pub fn query_migrate_data(deps: Deps) -> Result<Binary, ContractError> {
    let state = query_state(deps)?;
    let balances = query_balances(deps)?;
    let allowances = query_allowances(deps)?;
    Ok(to_json_binary(&VBalanceMigrateMsg {
        state,
        balances,
        allowances,
    })?)
}
