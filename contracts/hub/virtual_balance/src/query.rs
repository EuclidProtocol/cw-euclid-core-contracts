use cosmwasm_std::{to_json_binary, Binary, Deps, Uint128};
use cw_storage_plus::Bound;
use euclid::{
    chain::ChainUid,
    error::ContractError,
    msgs::virtual_balance::{
        GetBalanceResponse, GetStateResponse, GetUserBalancesResponse, GetUserBalancesResponseItem,
    },
    utils::pagination::Pagination,
    virtual_balance::BalanceKey,
};

use crate::state::{BALANCES, STATE};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetStateResponse { state })?)
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
    pagination: Option<Pagination<Uint128>>,
) -> Result<Binary, ContractError> {
    let Pagination {
        min,
        max,
        skip,
        limit,
    } = pagination.unwrap_or_default();

    let min = min.map(Bound::inclusive);
    let max = max.map(Bound::exclusive);

    let balances: Result<_, ContractError> = BALANCES
        .prefix((chain_uid.clone(), address.clone()))
        .range(deps.storage, min, max, cosmwasm_std::Order::Ascending)
        .skip(skip.unwrap_or(0) as usize)
        .take(limit.unwrap_or(10) as usize)
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
