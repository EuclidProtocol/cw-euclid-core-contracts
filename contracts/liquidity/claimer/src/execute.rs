use cosmwasm_std::{
    ensure, from_json, to_json_binary, DepsMut, Env, MessageInfo, Response, WasmMsg,
};
use euclid::{
    chain::CrossChainUser,
    error::ContractError,
    msgs::claimer::{
        Claim, ClaimVoucherData, CreateVoucherClaim, SignedTransaction, UpdateAdminMsg,
    },
};
use relayer::verify::{verify_signature, MsgSignData};

use crate::state::{CLAIMS, CLAIM_ID, SENDER_CLAIMS, STATE, USER_CLAIMS};

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

pub fn execute_create_voucher_claim(
    deps: &mut DepsMut,
    env: &Env,
    info: &MessageInfo,
    msg: CreateVoucherClaim,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Transfer tokens to contract to verify user has enough funds
    let transfer_vcoin_msg = euclid::msgs::factory::ExecuteMsg::TransferVirtualBalance {
        token: msg.token.clone(),
        amount: msg.amount,
        recipient_address: CrossChainUser::new(state.chain_uid, env.contract.address.to_string()),
        timeout: None,
    };

    let transfer_vcoin_msg = WasmMsg::Execute {
        contract_addr: state.factory_address.to_string(),
        msg: to_json_binary(&transfer_vcoin_msg)?,
        funds: vec![],
    };

    let mut response = Response::new();
    response = response.add_message(transfer_vcoin_msg);

    // Lets create a claim
    let claim_id = CLAIM_ID.load(deps.storage).unwrap_or(0u128); // Get latest claim id
    CLAIM_ID.save(
        deps.storage,
        &claim_id
            .checked_add(1u128)
            .ok_or(ContractError::new("Claim id overflow"))?,
    )?; // Increment claim id

    // Save claim
    let claim = Claim {
        token: msg.token.clone(),
        amount: msg.amount,
        claimer_pubkey: msg.claimer_pubkey.clone(),
        sender: info.sender.to_string(),
    };
    CLAIMS.save(deps.storage, claim_id, &claim)?;

    // Save sender claims
    let mut sender_claims = SENDER_CLAIMS
        .load(deps.storage, info.sender.to_string())
        .unwrap_or_default();

    sender_claims.push(claim_id);
    SENDER_CLAIMS.save(deps.storage, info.sender.to_string(), &sender_claims)?;

    // Save user claims
    let mut user_claims = USER_CLAIMS
        .load(deps.storage, msg.claimer_pubkey.to_string())
        .unwrap_or_default();

    user_claims.push(claim_id);
    USER_CLAIMS.save(deps.storage, msg.claimer_pubkey.to_string(), &user_claims)?;

    Ok(response)
}

pub fn execute_claim_voucher(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: SignedTransaction,
) -> Result<Response, ContractError> {
    let signed_data: MsgSignData = from_json(msg.data.clone())?;
    let first_msg = signed_data
        .msgs
        .first()
        .ok_or(ContractError::new("No messages found"))?
        .clone()
        .value;
    let claim_msg: ClaimVoucherData = from_json(first_msg.data.clone())?;

    let claim = CLAIMS.load(deps.storage, claim_msg.claim_id)?;

    // Verify signature
    let verified = verify_signature(
        deps.as_ref(),
        &msg.data,
        &msg.signature,
        &claim.claimer_pubkey,
    )?;
    ensure!(verified, ContractError::new("Invalid signature"));

    let state = STATE.load(deps.storage)?;

    ensure!(
        claim_msg.recipient.chain_uid == state.chain_uid,
        ContractError::new("Only same chain recipient is allowed")
    );

    let transfer_vcoin_msg = euclid::msgs::factory::ExecuteMsg::TransferVirtualBalance {
        token: claim.token.clone(),
        amount: claim.amount,
        recipient_address: claim_msg.recipient.clone(),
        timeout: None,
    };

    let transfer_vcoin_msg = WasmMsg::Execute {
        contract_addr: state.factory_address.to_string(),
        msg: to_json_binary(&transfer_vcoin_msg)?,
        funds: vec![],
    };

    // Remove claim from claims
    CLAIMS.remove(deps.storage, claim_msg.claim_id);

    // Not sure about these blocks as it will increase gas fee for user who is claiming if sender has too many claim messages

    // Remove claim from sender claims
    let mut sender_claims = SENDER_CLAIMS.load(deps.storage, claim.sender.clone())?;
    sender_claims.retain(|id| *id != claim_msg.claim_id);
    SENDER_CLAIMS.save(deps.storage, claim.sender.clone(), &sender_claims)?;

    // Remove claim from user claims
    let mut user_claims = USER_CLAIMS.load(deps.storage, claim.claimer_pubkey.to_string())?;
    user_claims.retain(|id| *id != claim_msg.claim_id);
    USER_CLAIMS.save(deps.storage, claim.claimer_pubkey.to_string(), &user_claims)?;

    Ok(Response::new()
        .add_message(transfer_vcoin_msg)
        .add_attribute("claim_id", claim_msg.claim_id.to_string())
        .add_attribute("sender", claim.sender)
        .add_attribute("recipient", claim_msg.recipient.to_sender_string())
        .add_attribute("token", claim.token.to_string())
        .add_attribute("claimer_pubkey", claim.claimer_pubkey.to_string())
        .add_attribute("claim_msg_sender", info.sender.to_string())
        .add_attribute("amount", claim.amount.to_string()))
}
