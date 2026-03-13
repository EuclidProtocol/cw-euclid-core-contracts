#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};

use cw2::set_contract_version;
use euclid::{admin::EuclidAdmin, error::ContractError};
use relayer::msgs::{ExecuteMsg, InstantiateMsg, QueryMsg, State};

use crate::{
    execute::{
        execute_add_validator, execute_meta_transaction, execute_remove_validator,
        execute_update_admin, execute_update_state,
    },
    query::{get_admin, get_nonce_relayed, get_state, get_validators},
    state::{ADMIN, STATE},
};

// version info for migration info
pub(crate) const CONTRACT_NAME: &str = "crates.io:euclid-relayer";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let admin = EuclidAdmin::default(info.sender);
    let state = State {
        message_signer: msg.message_signer,
        signature_threshold: msg.signature_threshold,
    };
    STATE.save(deps.storage, &state)?;
    ADMIN.save(deps.storage, &admin)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute(
            "message_signer_pubkey",
            state.message_signer.pubkey.to_string(),
        )
        .add_attribute(
            "message_signer_address",
            state.message_signer.address.to_string(),
        )
        .add_attribute("signature_threshold", msg.signature_threshold.to_string()))
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
            execute_meta_transaction(&mut deps, &env, &info, msg)
        }
        ExecuteMsg::UpdateState(msg) => execute_update_state(&mut deps, &info, msg),
        ExecuteMsg::UpdateAdmin(msg) => execute_update_admin(&mut deps, env, &info, msg),
        ExecuteMsg::AddValidator {
            validator,
            chain_uid,
        } => execute_add_validator(&mut deps, &info, validator, chain_uid),
        ExecuteMsg::RemoveValidator {
            validator,
            chain_uid,
        } => execute_remove_validator(&mut deps, &info, validator, chain_uid),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::GetState {} => Ok(to_json_binary(&get_state(&deps)?)?),
        QueryMsg::GetAdmin {} => Ok(to_json_binary(&get_admin(&deps)?)?),
        QueryMsg::NonceRelayed { nonce } => Ok(to_json_binary(&get_nonce_relayed(&deps, nonce)?)?),
        QueryMsg::Validators {} => Ok(to_json_binary(&get_validators(&deps)?)?),
    }
}
