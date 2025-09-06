use cosmwasm_std::{
    ensure, from_json, to_json_binary, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use euclid::{
    chain::{CrossChainUser, CrossChainUserWithLimit},
    error::ContractError,
    msgs::{
        claimer::{
            Claim, ClaimVoucherData, CreateVoucherClaim, SignedTransaction, UpdateAdminMsg,
            VirtualBalanceReceiveHookMsg,
        },
        hook::VirtualBalanceReceive,
    },
    token::Token,
};
use relayer::verify::{verify_signature, MsgSignData};

use crate::state::{CLAIMS, CLAIM_ID, STATE};

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

pub fn execute_virtual_balance_receive(
    deps: &mut DepsMut,
    _env: &Env,
    info: &MessageInfo,
    transfer_msg: VirtualBalanceReceive,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == state.vcoin_address,
        ContractError::new("Invalid vcoin address")
    );

    let claim_msg: VirtualBalanceReceiveHookMsg = from_json(transfer_msg.msg.clone())?;
    match claim_msg {
        VirtualBalanceReceiveHookMsg::CreateVoucherClaim(msg) => execute_create_voucher_claim(
            deps,
            &transfer_msg.sender,
            Token::create(transfer_msg.token_id)?,
            transfer_msg.amount,
            msg,
        ),
    }
}
pub fn execute_create_voucher_claim(
    deps: &mut DepsMut,
    sender: &CrossChainUser,
    token: Token,
    amount: Uint128,
    msg: CreateVoucherClaim,
) -> Result<Response, ContractError> {
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
        token: token.clone(),
        amount,
        claimer_pubkey: msg.claimer_pubkey.clone(),
        sender: sender.clone(),
        pseudo_claim_id: msg.pseudo_claim_id.clone(),
        claim_group_id: msg.claim_group_id.clone(),
    };
    CLAIMS.save(deps.storage, claim_id, &claim)?;

    Ok(Response::new()
        .add_attribute("create_claim", claim_id.to_string())
        .add_attribute("sender", sender.to_sender_string())
        .add_attribute("token", token.to_string())
        .add_attribute("claimer_pubkey", msg.claimer_pubkey.to_string())
        .add_attribute("pseudo_claim_id", msg.pseudo_claim_id.unwrap_or_default())
        .add_attribute("claim_group_id", msg.claim_group_id.unwrap_or_default())
        .add_attribute("amount", amount.to_string()))
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

    let mut response = Response::new()
        .add_attribute("execute_claim", claim_msg.claim_id.to_string())
        .add_attribute("recipient", claim_msg.recipient.to_sender_string())
        .add_attribute("claim_msg_sender", info.sender.to_string());

    if claim_msg.release_funds {
        let release_funds_msg = euclid::msgs::factory::ExecuteMsg::WithdrawVirtualBalance {
            token: claim.token.clone(),
            amount: claim.amount,
            cross_chain_addresses: vec![CrossChainUserWithLimit {
                user: claim_msg.recipient.clone(),
                preferred_token_type: None,
                refund_address: None,
                forwarding_message: claim_msg.release_msg,
                unsafe_refund_voucher_to_recipient: Some(true),
                vcoin_msg: None,
                limit: Some(euclid::chain::Limit::Equal(claim.amount)),
            }],
            timeout: None,
        };
        let release_funds_msg = WasmMsg::Execute {
            contract_addr: state.factory_address.to_string(),
            msg: to_json_binary(&release_funds_msg)?,
            funds: vec![],
        };
        response = response.add_message(release_funds_msg);
    } else {
        let transfer_vcoin_msg = euclid::msgs::factory::ExecuteMsg::TransferVirtualBalance {
            token: claim.token.clone(),
            amount: claim.amount,
            recipient_address: claim_msg.recipient.clone(),
            from: None,
            msg: None,
            timeout: None,
        };
        let transfer_vcoin_msg = WasmMsg::Execute {
            contract_addr: state.factory_address.to_string(),
            msg: to_json_binary(&transfer_vcoin_msg)?,
            funds: vec![],
        };
        response = response.add_message(transfer_vcoin_msg);
    }

    // Remove claim from claims
    CLAIMS.remove(deps.storage, claim_msg.claim_id);

    Ok(response)
}
