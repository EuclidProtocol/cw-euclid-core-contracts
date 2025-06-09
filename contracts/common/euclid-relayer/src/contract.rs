#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use cw2::set_contract_version;
use euclid::error::ContractError;
use relayer::msgs::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

use crate::{
    execute::{
        execute_execute_authorized_transaction, execute_execute_meta_transaction,
        execute_update_admin, execute_update_state,
    },
    query::{get_state, nonce_relayed},
    state::{AUTHORIZED_ADDRESSES, STATE},
};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:euclid-relayer";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        relayer_pubkey: msg.relayer_pubkey,
        relayer_address: msg.relayer_address.clone(),
        admin: info.sender,
    };
    STATE.save(deps.storage, &state)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    AUTHORIZED_ADDRESSES.save(deps.storage, &msg.authorized_addresses)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("relayer_address", msg.relayer_address))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ExecuteMetaTransaction(msg) => {
            execute_execute_meta_transaction(&mut deps, &env, &info, msg)
        }
        ExecuteMsg::ExecuteAuthorizedTransaction(msg) => {
            execute_execute_authorized_transaction(&mut deps, &env, &info, msg)
        }
        ExecuteMsg::UpdateState(msg) => execute_update_state(&mut deps, &info, msg),
        ExecuteMsg::UpdateAdmin(msg) => execute_update_admin(&mut deps, &info, msg),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => Ok(to_json_binary(&get_state(&deps)?)?),
        QueryMsg::NonceRelayed { nonce } => Ok(to_json_binary(&nonce_relayed(&deps, nonce)?)?),
    }
}
