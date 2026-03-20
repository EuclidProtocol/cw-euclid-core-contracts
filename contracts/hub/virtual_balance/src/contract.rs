#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;
use euclid::admin::EuclidAdmin;

use crate::execute::{
    execute_approve, execute_burn, execute_deregister_token_metadata, execute_mint,
    execute_register_token_metadata, execute_remove_zero_state_values, execute_transfer,
    execute_update_admin, execute_update_router,
};
use crate::query::{
    query_admin, query_all_balances, query_all_escrow_balances, query_all_token_metadata,
    query_balance, query_escrow_balance, query_state, query_token_balances, query_token_escrows,
    query_token_metadata, query_token_metadata_by_denom, query_token_registered,
    query_user_balances,
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
        ExecuteMsg::Transfer(msg) => execute_transfer(&mut deps, env, info, msg),
        ExecuteMsg::UpdateAdmin {
            new_admin,
            admin_type,
        } => execute_update_admin(deps, env, info, new_admin, admin_type),
        ExecuteMsg::UpdateRouter { router } => execute_update_router(deps, info, router),
        ExecuteMsg::Approve(msg) => execute_approve(deps, info, msg),
        ExecuteMsg::RemoveZeroStateValues { start_after, limit } => {
            execute_remove_zero_state_values(deps, info, start_after, limit)
        }
        ExecuteMsg::RegisterTokenMetadata { token_metadata } => {
            execute_register_token_metadata(deps, info, token_metadata)
        }
        ExecuteMsg::DeregisterTokenMetadata {
            token_id,
            chain_uid,
            token_type,
        } => execute_deregister_token_metadata(deps, info, token_id, chain_uid, token_type),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => query_state(deps),
        QueryMsg::GetAdmin {} => query_admin(deps),
        QueryMsg::GetBalance { balance_key } => query_balance(deps, balance_key),
        QueryMsg::GetUserBalances { user, pagination } => {
            query_user_balances(deps, user.chain_uid, user.address, pagination)
        }
        QueryMsg::GetAllBalances { pagination } => query_all_balances(deps, pagination),
        QueryMsg::GetTokenBalances {
            token_id,
            pagination,
        } => query_token_balances(deps, token_id, pagination),
        QueryMsg::GetEscrowBalance {
            token_id,
            chain_uid,
            token_type,
        } => query_escrow_balance(deps, token_id, chain_uid, token_type),
        QueryMsg::GetTokenEscrows {
            token_id,
            pagination,
        } => query_token_escrows(deps, token_id, pagination),
        QueryMsg::GetAllEscrowBalances { pagination } => {
            query_all_escrow_balances(deps, pagination)
        }
        QueryMsg::GetTokenMetadataByDenom {
            token_id,
            chain_uid,
            token_type,
        } => query_token_metadata_by_denom(deps, token_id, chain_uid, token_type),
        QueryMsg::GetTokenMetadata {
            token_id,
            pagination,
        } => query_token_metadata(deps, token_id, pagination),
        QueryMsg::GetAllTokenMetadata { pagination } => query_all_token_metadata(deps, pagination),
        QueryMsg::GetTokenRegistered { token_id } => query_token_registered(deps, token_id),
    }
}
