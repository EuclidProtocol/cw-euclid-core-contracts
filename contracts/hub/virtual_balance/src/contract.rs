#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;
use euclid::admin::EuclidAdmin;

use crate::execute::{
    execute_approve, execute_burn, execute_deregister_token_metadata, execute_mint,
    execute_register_token_metadata, execute_remove_zero_state_values, execute_transfer,
    execute_update_admin, execute_update_router,
};
use crate::query::{
    query_admin, query_all_balances, query_all_escrow_balances, query_all_token_metadata,
    query_allowance, query_balance, query_escrow_balance, query_state, query_token_balances,
    query_token_escrows, query_token_metadata, query_token_metadata_by_denom, query_token_status,
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
    euclid::build_info::set_build_info(deps.storage)?;

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
        .add_attribute("router", state.router)
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
        QueryMsg::GetAllowance { balance_key } => query_allowance(deps, balance_key),
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
        QueryMsg::GetTokenStatus { token_id } => query_token_status(deps, token_id),
        QueryMsg::GetBuildInfo {} => Ok(to_json_binary(&euclid::build_info::build_info(
            deps.storage,
            CONTRACT_VERSION,
        ))?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::helpers::{init, MockDeps};
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{attr, from_json};
    use euclid::admin::EuclidAdmin;
    use euclid::msgs::virtual_balance::msg::{InstantiateMsg, State};

    // -------------------------------------------------------------------------
    // instantiate
    // -------------------------------------------------------------------------

    #[test]
    fn test_instantiate_writes_state_and_default_admin() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);

        // Response carries expected attributes
        assert!(res
            .attributes
            .iter()
            .any(|a| *a == attr("method", "instantiate")));

        // State router is the info.sender (addr_make("router"))
        let router = deps.api.addr_make("router");
        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.router, router);

        // Admin defaults to all-same as router
        let admin = ADMIN.load(&deps.storage).unwrap();
        let expected_admin = EuclidAdmin::default(router);
        assert_eq!(admin, expected_admin);
    }

    #[test]
    fn test_instantiate_with_explicit_admin() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let router = deps.api.addr_make("router");
        let general = deps.api.addr_make("g_admin");
        let fee = deps.api.addr_make("f_admin");
        let migration = deps.api.addr_make("m_admin");

        let explicit_admin = EuclidAdmin::new(general.clone(), fee.clone(), migration.clone());
        let msg = InstantiateMsg {
            router: router.clone(),
            admin: Some(explicit_admin.clone()),
        };
        let info = cosmwasm_std::testing::message_info(&router, &[]);
        instantiate(deps.as_mut(), env, info, msg).unwrap();

        let saved = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(saved, explicit_admin);
    }

    #[test]
    fn test_instantiate_no_messages_emitted() {
        let mut deps = mock_dependencies();
        let res = init(&mut deps);
        assert_eq!(res.messages.len(), 0);
    }

    // -------------------------------------------------------------------------
    // query dispatch – smoke tests (exhaustive coverage lives in query.rs tests)
    // -------------------------------------------------------------------------

    #[test]
    fn test_query_get_state_returns_router() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let bin = query(deps.as_ref(), env, QueryMsg::GetState {}).unwrap();
        let state: State = from_json(&bin).unwrap();
        assert_eq!(state.router, deps.api.addr_make("router"));
    }

    #[test]
    fn test_query_get_admin_returns_admin() {
        let mut deps: MockDeps = mock_dependencies();
        init(&mut deps);
        let env = mock_env();

        let bin = query(deps.as_ref(), env, QueryMsg::GetAdmin {}).unwrap();
        let admin: EuclidAdmin = from_json(&bin).unwrap();
        let router = deps.api.addr_make("router");
        assert_eq!(admin, EuclidAdmin::default(router));
    }
}
