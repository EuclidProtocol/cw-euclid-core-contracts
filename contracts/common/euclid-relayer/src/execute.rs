use cosmwasm_std::{
    ensure, from_json, DepsMut, Env, MessageInfo, Response, Timestamp, Uint128, WasmMsg,
};
use euclid::{chain::ChainUid, error::ContractError};
use relayer::{
    msgs::{MetaTransaction, UpdateAdminMsg, UpdateStateMsg},
    verify::verify_signature,
    MetaTransactionData, Validator,
};

use crate::state::{NONCES, STATE, VALIDATORS};

pub fn execute_update_state(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: UpdateStateMsg,
) -> Result<Response, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let mut response = Response::new();
    if let Some(message_signer) = msg.message_signer {
        state.message_signer = message_signer.clone();
        response = response
            .add_attribute(
                "message_signer_pubkey_old_value",
                state.message_signer.pubkey.to_string(),
            )
            .add_attribute(
                "message_signer_pubkey_new_value",
                message_signer.pubkey.to_string(),
            )
            .add_attribute(
                "message_signer_address_old_value",
                state.message_signer.address.to_string(),
            )
            .add_attribute(
                "message_signer_address_new_value",
                message_signer.address.to_string(),
            );
    }

    if let Some(signature_threshold) = msg.signature_threshold {
        state.signature_threshold = signature_threshold;
        response = response
            .add_attribute(
                "signature_threshold_old_value",
                state.signature_threshold.to_string(),
            )
            .add_attribute(
                "signature_threshold_new_value",
                signature_threshold.to_string(),
            );
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

pub fn execute_meta_transaction(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    msg: MetaTransaction,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let meta_transaction: MetaTransactionData = from_json(msg.data.clone())?;
    // Ensure the nonce is not used
    ensure!(
        !NONCES.has(deps.storage, meta_transaction.nonce.clone()),
        ContractError::new(format!("Nonce already used: {}", meta_transaction.nonce).as_str())
    );
    // Save the nonce
    NONCES.save(
        deps.storage,
        meta_transaction.nonce.clone(),
        &Uint128::from(env.block.height),
    )?;

    // Ensure the timestamp is not exceeded
    ensure!(
        env.block.time <= Timestamp::from_seconds(msg.expiry),
        ContractError::new("Timestamp limit exceeded")
    );

    let verified = verify_signature(
        deps.as_ref(),
        &expiry_call_data(&msg.data, msg.expiry, msg.chain_uid.as_str()),
        &msg.admin_signature,
        &state.message_signer.pubkey,
    )?;

    ensure!(verified, ContractError::new("Invalid admin signature"));
    let validators = VALIDATORS
        .load(deps.storage, msg.chain_uid.clone())
        .map_err(|_| ContractError::new("Validators not found for chain"))?;
    let mut visited = vec![false; validators.len()];
    let mut valid_signatures = 0;
    for signature in msg.validator_signatures {
        let validator_index = validators
            .iter()
            .position(|v| v.pubkey == signature.pubkey)
            .ok_or(ContractError::new("Validator not found"))?;
        if visited[validator_index] {
            continue;
        }
        let verified = verify_signature(
            deps.as_ref(),
            &expiry_call_data(&msg.data, signature.expiry, msg.chain_uid.as_str()),
            &signature.signature,
            &signature.pubkey,
        )?;
        if !verified {
            continue;
        }
        valid_signatures += 1;
        visited[validator_index] = true;
    }

    ensure!(
        valid_signatures >= state.signature_threshold,
        ContractError::new(
            format!(
                "Threshold not met: expected {}, got {}",
                state.signature_threshold, valid_signatures
            )
            .as_str()
        )
    );

    let relay_msg = WasmMsg::Execute {
        contract_addr: meta_transaction.target.to_string(),
        msg: meta_transaction.call_data,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(relay_msg)
        .add_attribute("relayer_nonce", meta_transaction.nonce)
        .add_attribute("relayer_target", meta_transaction.target)
        .add_attribute("relayer_sender", info.sender.to_string()))
}

fn expiry_call_data(data: &str, expiry: u64, chain_uid: &str) -> String {
    let expiry_call_data = format!(
        "{data},{expiry},{chain_uid}",
        data = data,
        expiry = expiry,
        chain_uid = chain_uid
    );
    expiry_call_data
}

pub fn execute_add_validator(
    deps: &mut DepsMut,
    info: &MessageInfo,
    validator: Validator,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let mut validators = VALIDATORS
        .load(deps.storage, chain_uid.clone())
        .unwrap_or(vec![]);
    ensure!(
        !validators.contains(&validator),
        ContractError::new("Validator already exists")
    );
    validators.push(validator.clone());
    VALIDATORS.save(deps.storage, chain_uid, &validators)?;
    Ok(Response::new().add_attribute("validator_added", validator.address.to_string()))
}

pub fn execute_remove_validator(
    deps: &mut DepsMut,
    info: &MessageInfo,
    validator: Validator,
    chain_uid: ChainUid,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});
    let mut validators = VALIDATORS
        .load(deps.storage, chain_uid.clone())
        .unwrap_or(vec![]);
    let index = validators
        .iter()
        .position(|v| v.address == validator.address);
    if let Some(index) = index {
        validators.remove(index);
    } else {
        return Err(ContractError::new("Validator does not exist"));
    }
    VALIDATORS.save(deps.storage, chain_uid, &validators)?;
    Ok(Response::new().add_attribute("validator_removed", validator.address.clone()))
}
