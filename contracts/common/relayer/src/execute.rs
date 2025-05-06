use cosmwasm_std::{ensure, to_json_string, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg};
use euclid::error::ContractError;
use relayer::{
    msgs::{MetaTransaction, UpdateAdminMsg, UpdateStateMsg},
    verify::{verify_signature, MsgSignData, MsgSignDataMsg, MsgSignDataValue},
};

use crate::state::{NONCES, STATE};

pub fn execute_update_state(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: UpdateStateMsg,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let mut response = Response::new();
    if let Some(relayer_pubkey) = msg.relayer_pubkey {
        state.relayer_pubkey = relayer_pubkey.clone();
        response = response
            .add_attribute("relayer_pubkey_old_value", state.relayer_pubkey.to_string())
            .add_attribute("relayer_pubkey_new_value", relayer_pubkey.to_string());
    }
    if let Some(relayer_address) = msg.relayer_address {
        state.relayer_address = relayer_address.clone();
        response = response
            .add_attribute("relayer_address_old_value", state.relayer_address.clone())
            .add_attribute("relayer_address_new_value", relayer_address);
    }
    STATE.save(deps.storage, &state)?;
    Ok(response)
}

pub fn execute_update_admin(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: UpdateAdminMsg,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    // Ensure the sender is the current admin
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    deps.api.addr_validate(msg.new_admin.as_str())?;

    state.admin = msg.new_admin.clone();
    STATE.save(deps.storage, &state)?;
    Ok(Response::new()
        .add_attribute("old_admin", state.admin.to_string())
        .add_attribute("new_admin", msg.new_admin.to_string()))
}

pub fn execute_execute_meta_transaction(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    msg: MetaTransaction,
) -> Result<Response, ContractError> {
    // Ensure the nonce is not used
    ensure!(
        !NONCES.has(deps.storage, info.sender.to_string()),
        ContractError::new("Nonce already used")
    );
    // Save the nonce
    NONCES.save(
        deps.storage,
        msg.nonce.clone(),
        &Uint128::from(env.block.height),
    )?;

    let state = STATE.load(deps.storage)?;

    // Create signed data and verify signature against it
    let signed_data_msg = MsgSignDataMsg::new(MsgSignDataValue::new(
        msg.call_data.clone(),
        state.relayer_address.clone(),
    ));
    let signed_data = MsgSignData::new(vec![signed_data_msg]);
    let signed_data = to_json_string(&signed_data)?;

    let verified = verify_signature(
        deps.as_ref(),
        signed_data,
        msg.signature.as_slice(),
        &state.relayer_pubkey,
    )?;

    ensure!(verified, ContractError::new("Invalid signature"));

    let relay_msg = WasmMsg::Execute {
        contract_addr: msg.target.to_string(),
        msg: msg.call_data,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(relay_msg)
        .add_attribute("relayer_nonce", msg.nonce)
        .add_attribute("relayer_target", msg.target))
}
