#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;
use euclid::admin::EuclidAdmin;

use crate::execute::{
    execute_approve, execute_burn, execute_mint, execute_normalize_balance_keys,
    execute_remove_zero_state_values, execute_transfer, execute_update_admin,
    execute_update_router,
};
use crate::query::{
    query_admin, query_all_balances, query_allowance, query_balance, query_state,
    query_token_balances, query_user_balances,
};
use crate::state::{ADMIN, STATE};
use euclid::error::ContractError;
use euclid::msgs::virtual_balance::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:virtual_balance";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let admin = msg
        .admin
        .unwrap_or(EuclidAdmin::default(info.sender.clone()));
    let state = State {
        router: info.sender,
    };

    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &admin)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("token_balance_address", env.contract.address)
        .add_attribute("admin", admin.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Mint(msg) => execute_mint(deps, info, msg),
        ExecuteMsg::Burn(msg) => execute_burn(deps, info, msg),
        ExecuteMsg::Transfer(msg) => execute_transfer(&mut deps, info, msg),
        ExecuteMsg::UpdateAdmin {
            new_admin,
            admin_type,
        } => execute_update_admin(deps, env, info, new_admin, admin_type),
        ExecuteMsg::UpdateRouter { router } => execute_update_router(deps, info, router),
        ExecuteMsg::Approve(msg) => execute_approve(deps, info, msg),
        ExecuteMsg::RemoveZeroStateValues { start_after, limit } => {
            execute_remove_zero_state_values(deps, info, start_after, limit)
        }
        ExecuteMsg::NormalizeBalanceKeys { skip, limit } => {
            execute_normalize_balance_keys(deps, info, skip, limit)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetAdmin {} => query_admin(deps),
        QueryMsg::GetBalance { balance_key } => query_balance(deps, balance_key),
        QueryMsg::GetAllowance { balance_key } => query_allowance(deps, balance_key),
        QueryMsg::GetUserBalances { user, pagination } => {
            query_user_balances(deps, user.chain_uid, user.address, pagination)
        }
        QueryMsg::GetAllBalances { pagination } => query_all_balances(deps, pagination),
        QueryMsg::GetTokenBalances {
            token_id,
            pagination,
        } => query_token_balances(deps, token_id, pagination),
    }
}
