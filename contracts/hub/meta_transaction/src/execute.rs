use cosmwasm_std::{
    ensure, from_json, DepsMut, Env, MessageInfo, Response, Timestamp, Uint128, WasmMsg,
};
use euclid::error::ContractError;
use euclid::msgs::meta_transaction::{
    AuthorizedTransaction, MetaTransaction, MetaTransactionData, UpdateAdminMsg, UpdateStateMsg,
};
use relayer::verify::{verify_signature, MsgSignData};

use crate::state::{AUTHORIZED_ADDRESSES, NONCES, STATE};

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
            .add_attribute(
                "meta_transaction_pubkey_old_value",
                state.relayer_pubkey.to_string(),
            )
            .add_attribute(
                "meta_transaction_pubkey_new_value",
                relayer_pubkey.to_string(),
            );
    }
    if let Some(relayer_address) = msg.relayer_address {
        state.relayer_address = relayer_address.clone();
        response = response
            .add_attribute(
                "meta_transaction_address_old_value",
                state.relayer_address.clone(),
            )
            .add_attribute("meta_transaction_address_new_value", relayer_address);
    }

    if let Some(authorized_addresses) = msg.authorized_addresses {
        AUTHORIZED_ADDRESSES.save(deps.storage, &authorized_addresses)?;
        response = response.add_attribute("updated_authorized_addresses", "true");
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
    let state = STATE.load(deps.storage)?;

    let signed_data: MsgSignData = from_json(msg.data.clone())?;
    let first_msg = signed_data
        .msgs
        .first()
        .ok_or(ContractError::new("No messages found"))?
        .clone()
        .value;
    let meta_transaction: MetaTransactionData = from_json(first_msg.data.clone())?;

    ensure!(
        first_msg.signer == state.relayer_address,
        ContractError::Generic {
            err: format!(
                "Invalid signer: expected {}, got {}",
                state.relayer_address, first_msg.signer
            )
        }
    );
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
        env.block.time <= Timestamp::from_seconds(meta_transaction.expiry),
        ContractError::new("Timestamp limit exceeded")
    );

    let verified = verify_signature(
        deps.as_ref(),
        &msg.data,
        &msg.signature,
        &state.relayer_pubkey,
    )?;

    ensure!(verified, ContractError::new("Invalid signature"));

    let relay_msg = WasmMsg::Execute {
        contract_addr: state.router_contract.to_string(),
        msg: meta_transaction.call_data,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(relay_msg)
        .add_attribute("relayer_nonce", meta_transaction.nonce)
        .add_attribute("relayer_sender", info.sender.to_string()))
}

pub fn execute_execute_authorized_transaction(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    msg: AuthorizedTransaction,
) -> Result<Response, ContractError> {
    let authorized_addresses = AUTHORIZED_ADDRESSES.load(deps.storage).unwrap_or_default();
    ensure!(
        authorized_addresses.contains(&info.sender),
        ContractError::Unauthorized {}
    );
    // Ensure the nonce is not used
    ensure!(
        !NONCES.has(deps.storage, msg.nonce.clone()),
        ContractError::new(format!("Nonce already used: {}", msg.nonce).as_str())
    );
    // Save the nonce
    NONCES.save(
        deps.storage,
        msg.nonce.clone(),
        &Uint128::from(env.block.height),
    )?;

    let router_contract = STATE.load(deps.storage)?.router_contract;
    let relay_msg = WasmMsg::Execute {
        contract_addr: router_contract.to_string(),
        msg: msg.call_data,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(relay_msg)
        .add_attribute("meta_transaction_nonce", msg.nonce)
        .add_attribute("meta_transaction_target", router_contract)
        .add_attribute("meta_transaction_sender", info.sender.to_string()))
}
