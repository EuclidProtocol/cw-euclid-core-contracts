#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, Uint128,
};

use euclid::error::ContractError;
use secret_cw2::set_contract_version;

// use cw2::set_contract_version;

use crate::execute::{
    self, execute_add_allowed_denom, execute_deposit_native, execute_disallow_denom,
    execute_withdraw, receive_snip20,
};
use crate::query::{self, query_token_id};
use crate::state::{State, STATE};

use euclid::msgs::escrow::{EscrowInstantiateResponse, ExecuteMsg, InstantiateMsg, QueryMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:escrow";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        token_id: msg.token_id.clone(),
        // Set the sender as the factory address, since we want the factory to instantiate the escrow.
        factory_address: info.sender.clone(),
        total_amount: Uint128::zero(),
    };
    STATE.save(deps.storage, &state)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let data = EscrowInstantiateResponse {
        token: msg.token_id.clone(),
        address: env.contract.address.to_string(),
        code_hash: env.contract.code_hash.clone(),
    };
    let mut res = Response::new();

    if let Some(denom) = msg.allowed_denom {
        res = execute::execute_add_allowed_denom(deps, env, info.clone(), denom)?;
    }

    Ok(res
        .add_attribute("method", "instantiate")
        .add_attribute("token_id", msg.token_id.as_str())
        .add_attribute("factory_address", info.sender)
        .set_data(to_binary(&data)?))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::DepositNative {} => execute_deposit_native(deps, env, info),
        ExecuteMsg::AddAllowedDenom { denom } => execute_add_allowed_denom(deps, env, info, denom),
        ExecuteMsg::DisallowDenom { denom } => execute_disallow_denom(deps, env, info, denom),
        ExecuteMsg::Receive(msg) => receive_snip20(deps, env, info, msg),
        ExecuteMsg::Withdraw {
            recipient,
            amount,
            memo,
            decoys,
            entropy,
            padding,
        } => execute_withdraw(
            deps, env, info, recipient, amount, memo, decoys, entropy, padding,
        ),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::State {} => query::query_state(deps),
        QueryMsg::TokenId {} => query_token_id(deps),
        QueryMsg::TokenAllowed { denom } => query::query_token_allowed(deps, denom),
        QueryMsg::AllowedDenoms {} => query::query_allowed_denoms(deps),
    }
}
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(_deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let id = msg.id;
    Err(ContractError::Std(StdError::generic_err(format!(
        "Unknown reply id: {}",
        id
    ))))
}

#[cfg(test)]
mod tests {}
