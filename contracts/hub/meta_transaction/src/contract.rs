#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use cw2::set_contract_version;
use euclid::admin::EuclidAdmin;
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

use crate::execute::execute_meta_transaction;
use crate::query::get_nonce;
use crate::{
    execute::execute_update_admin,
    query::get_state,
    state::{ADMIN, STATE},
};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:meta-transaction";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        router_contract: msg.router_contract.clone(),
    };
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &EuclidAdmin::default(info.sender))?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("router_contract", msg.router_contract))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateAdmin(msg) => execute_update_admin(&mut deps, env, &info, msg),
        ExecuteMsg::ExecuteMetaTransaction(msg) => {
            execute_meta_transaction(&mut deps, &env, &info, msg)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => Ok(to_json_binary(&get_state(&deps)?)?),
        QueryMsg::NonceRelayed { nonce } => Ok(to_json_binary(&get_nonce(&deps, nonce)?)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ADMIN, STATE};
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{attr, Addr};
    use euclid::admin::EuclidAdmin;
    use euclid::msgs::meta_transaction::msg::InstantiateMsg;

    fn router_contract() -> Addr {
        Addr::unchecked("router")
    }

    fn make_instantiate_msg() -> InstantiateMsg {
        InstantiateMsg {
            router_contract: router_contract(),
        }
    }

    #[test]
    fn test_instantiate_stores_state_and_admin() {
        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("creator");
        let info = message_info(&sender, &[]);

        let res = instantiate(
            deps.as_mut(),
            mock_env(),
            info,
            make_instantiate_msg(),
        )
        .unwrap();

        assert_eq!(res.attributes[0], attr("method", "instantiate"));
        assert_eq!(
            res.attributes[1],
            attr("router_contract", router_contract().as_str())
        );

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.router_contract, router_contract());

        let admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(admin, EuclidAdmin::default(sender));
    }

    #[test]
    fn test_instantiate_sets_cw2_contract_version() {
        use cw2::get_contract_version;

        let mut deps = mock_dependencies();
        let sender = deps.api.addr_make("creator");
        let info = message_info(&sender, &[]);

        instantiate(deps.as_mut(), mock_env(), info, make_instantiate_msg()).unwrap();

        let version = get_contract_version(&deps.storage).unwrap();
        assert_eq!(version.contract, CONTRACT_NAME);
        assert_eq!(version.version, CONTRACT_VERSION);
    }
}
