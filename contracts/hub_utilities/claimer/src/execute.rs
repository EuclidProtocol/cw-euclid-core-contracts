use cosmwasm_std::{
    ensure, from_json, to_json_binary, DepsMut, Env, MessageInfo, Response, Uint256, WasmMsg,
};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{
        claimer::{
            msg::{Claim, ClaimVoucherData, SignedTransaction, UpdateAdminMsg},
            voucher_receive::{CreateVoucherClaim, VoucherReceiveHookMsg},
        },
        hook::VoucherReceive,
    },
    token::Token,
};
use relayer::verify::{verify_signature, MsgSignData};

use crate::state::{ADMIN, CLAIMS, CLAIM_ID, STATE};

pub fn execute_update_admin(
    deps: &mut DepsMut,
    info: &MessageInfo,
    msg: UpdateAdminMsg,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(info)?;
    let current_admin = ADMIN.load(deps.storage)?;
    // Ensure the sender is the current admin
    ensure!(info.sender == current_admin, ContractError::Unauthorized {});

    let new_admin = deps.api.addr_validate(msg.new_admin.as_str())?;

    ADMIN.save(deps.storage, &new_admin)?;
    Ok(Response::new()
        .add_attribute("old_admin", current_admin.to_string())
        .add_attribute("new_admin", new_admin.to_string()))
}

pub fn execute_virtual_balance_receive(
    deps: &mut DepsMut,
    _env: &Env,
    info: &MessageInfo,
    transfer_msg: VoucherReceive,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(info)?;
    let state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == state.vcoin_address,
        ContractError::new("Invalid vcoin address")
    );

    let claim_msg: VoucherReceiveHookMsg = from_json(transfer_msg.msg.clone())?;
    match claim_msg {
        VoucherReceiveHookMsg::CreateVoucherClaim(msg) => {
            let amount: Uint256 = transfer_msg.amount;
            execute_create_voucher_claim(
                deps,
                &transfer_msg.sender,
                Token::create(transfer_msg.token_id)?,
                amount,
                msg,
            )
        }
    }
}
pub fn execute_create_voucher_claim(
    deps: &mut DepsMut,
    sender: &CrossChainUser,
    token: Token,
    amount: Uint256,
    msg: CreateVoucherClaim,
) -> Result<Response, ContractError> {
    // Reject mixed-case or empty addresses before storing the claim
    sender.validate()?;
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
    cw_utils::nonpayable(info)?;
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
        !claim_msg.recipients.is_empty(),
        ContractError::new("Empty recipient")
    );

    let mut response = Response::new()
        .add_attribute("execute_claim", claim_msg.claim_id.to_string())
        .add_attribute("claim_msg_sender", info.sender.to_string());

    for recipient in claim_msg.recipients.iter() {
        let unsafe_allowed = recipient.unsafe_refund_as_voucher.unwrap_or(false);
        ensure!(
            unsafe_allowed,
            ContractError::new(
                "Unsafe refund as voucher is important criteria for claim release fail"
            )
        );
    }

    let transfer_voucher_msg = euclid::msgs::router::ExecuteMsg::TransferVoucher {
        token: claim.token.clone(),
        amount: claim.amount,
        recipient: claim_msg.recipients.clone(),
    };
    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: state.router_contract.to_string(),
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };
    response = response.add_message(transfer_voucher_msg);

    // Remove claim from claims
    CLAIMS.remove(deps.storage, claim_msg.claim_id);

    Ok(response)
}
