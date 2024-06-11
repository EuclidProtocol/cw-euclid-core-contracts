use cosmwasm_std::{to_json_binary, Binary, Deps, Uint128};
use euclid::{
    error::ContractError,
    msgs::vcoin::{GetBalanceResponse, GetStateResponse, GetUserBalancesResponse},
    vcoin::BalanceKey,
};

use crate::state::{SNAPSHOT_BALANCES, STATE};

pub fn query_state(deps: Deps) -> Result<Binary, ContractError> {
    let state = STATE.load(deps.storage)?;
    Ok(to_json_binary(&GetStateResponse { state })?)
}

pub fn query_balance(deps: Deps, balance_key: BalanceKey) -> Result<Binary, ContractError> {
    let balance = SNAPSHOT_BALANCES.may_load(
        deps.storage,
        balance_key.clone().to_serialized_balance_key(),
    )?;
    Ok(to_json_binary(&GetBalanceResponse {
        amount: balance.unwrap_or(Uint128::zero()),
        balance_key,
    })?)
}

pub fn query_user_balances(
    deps: Deps,
    chain_id: String,
    address: String,
) -> Result<Binary, ContractError> {
    let balances: Result<_, ContractError> = SNAPSHOT_BALANCES
        .prefix((chain_id.clone(), address.clone()))
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|res| {
            let res = res?;
            Ok(GetBalanceResponse {
                balance_key: BalanceKey {
                    chain_id: chain_id.clone(),
                    address: address.clone(),
                    token_id: res.0,
                },
                amount: res.1,
            })
        })
        .collect();

    Ok(to_json_binary(&GetUserBalancesResponse {
        balances: balances?,
    })?)
}
