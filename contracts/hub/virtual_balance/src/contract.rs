#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;

use crate::execute::{
    execute_approve, execute_burn, execute_mint, execute_remove_zero_state_values,
    execute_transfer, execute_update_state,
};
use crate::query::{query_allowance, query_balance, query_state, query_user_balances};
use crate::state::STATE;
use euclid::error::ContractError;
use euclid::msgs::virtual_balance::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:virtual_balance";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let state = State {
        router: info.sender.to_string(),
        admin: msg.admin.unwrap_or(info.sender),
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("token_balance_address", env.contract.address)
        .add_attribute("admin", state.admin))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Mint(msg) => execute_mint(deps, info, msg),
        ExecuteMsg::Burn(msg) => execute_burn(deps, info, msg),
        ExecuteMsg::Transfer(msg) => execute_transfer(&mut deps, info, msg),
        ExecuteMsg::UpdateState { router, admin } => {
            execute_update_state(deps, info, router, admin)
        }
        ExecuteMsg::Approve(msg) => execute_approve(deps, info, msg),
        ExecuteMsg::RemoveZeroStateValues { start_after, limit } => {
            execute_remove_zero_state_values(deps, info, start_after, limit)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetBalance { balance_key } => query_balance(deps, balance_key),
        QueryMsg::GetUserBalances { user } => {
            query_user_balances(deps, user.chain_uid, user.address)
        }
        QueryMsg::GetAllowance { balance_key } => query_allowance(deps, balance_key),
    }
}
