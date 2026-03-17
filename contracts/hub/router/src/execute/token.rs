use cosmwasm_std::{
    ensure, to_json_binary, to_json_string, Binary, DepsMut, Env, Response, SubMsg, Uint128,
    WasmMsg,
};
use euclid::{
    cross_chain_user::CrossChainUser,
    error::ContractError,
    limit::Limit,
    msgs::{cross_chain_config::CrossChainConfig, router::TokenDenom},
    recipient::Recipient,
    token::Token,
    utils::tx::generate_tx,
    voucher::BalanceKey,
};
use euclid_ibc::factory_ibc::FactoryCrossChainExecuteMsg;

use euclid::msgs::virtual_balance::msg::{ExecuteBurn, ExecuteMint, GetEscrowBalanceResponse};

use crate::{
    helpers::release::get_release_fee_storage,
    state::{
        PendingReleaseVoucher, CHAIN_TIMEOUT_SECONDS, CHAIN_UID_TO_CHAIN, LOCKED_CHAINS,
        PENDING_RELEASE_VOUCHER, TOKEN_DENOMS, VIRTUAL_BALANCE_CONTRACT,
    },
};

pub fn execute_withdraw_voucher(
    deps: &mut DepsMut,
    env: Env,
    sender: CrossChainUser,
    token: Token,
    amount: Uint128,
    recipient: Recipient,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?.into_string();
    let available_denoms = TOKEN_DENOMS.load(deps.storage, token.clone())?;
    let ack_response = cross_chain_config.ack_response;
    let timeout = cross_chain_config.timeout;
    let tx_id = generate_tx(deps, &env, &sender)?;
    let (msgs, released_amount) = _release_voucher(
        deps,
        &env,
        virtual_balance_address,
        available_denoms,
        sender,
        token,
        amount,
        recipient,
        ack_response,
        timeout,
        tx_id,
    )?;
    Ok(Response::new()
        .add_submessages(msgs)
        .add_attribute("released_amount", released_amount.to_string())
        .add_attribute("method", "withdraw_voucher"))
}

pub fn execute_transfer_voucher(
    deps: &mut DepsMut,
    env: Env,
    sender: CrossChainUser,
    token: Token,
    amount: Uint128,
    recipients: Vec<Recipient>,
) -> Result<Response, ContractError> {
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?.into_string();

    let mut response = Response::new().add_attribute("recipients", to_json_string(&recipients)?);

    let mut remaining_withdraw_amount = amount;

    let mut recipients_iterator = recipients.into_iter().peekable();
    let available_denoms = TOKEN_DENOMS.load(deps.storage, token.clone())?;

    let mut release_initiated_amount = Uint128::zero();
    let mut transferred_amount = Uint128::zero();

    let mut index = 0;

    // Ensure that the amount desired doesn't exceed the current balance
    while !remaining_withdraw_amount.is_zero() && recipients_iterator.peek().is_some() {
        let recipient = recipients_iterator
            .next()
            .ok_or(ContractError::new("Recipient Iter Failed"))?;

        let release_amount_event_key = format!("release_id_{}_amount", index);
        let transfer_amount_event_key = format!("transfer_id_{}_amount", index);

        // We will transfer vouchers to the recipient
        if recipient.denom.is_voucher() {
            let (transfer_voucher_msgs, transfer_amount) = _transfer_voucher_as_voucher(
                virtual_balance_address.clone(),
                sender.clone(),
                token.clone(),
                remaining_withdraw_amount,
                recipient.clone(),
            )?;
            if transfer_amount.is_zero() {
                continue;
            }
            if !transfer_voucher_msgs.is_empty() {
                response = response.add_submessages(transfer_voucher_msgs);
            }
            response =
                response.add_attribute(transfer_amount_event_key, transfer_amount.to_string());
            remaining_withdraw_amount = remaining_withdraw_amount.checked_sub(transfer_amount)?;
            transferred_amount = transferred_amount.checked_add(transfer_amount)?;
        } else {
            let tx_id = generate_tx(deps, &env, &sender.clone())?;
            let timeout = CHAIN_TIMEOUT_SECONDS
                .may_load(deps.storage, recipient.recipient.chain_uid.clone())?;
            let (release_msgs, release_amount) = _release_voucher(
                deps,
                &env,
                virtual_balance_address.clone(),
                available_denoms.clone(),
                sender.clone(),
                token.clone(),
                remaining_withdraw_amount,
                recipient.clone(),
                None,
                timeout,
                tx_id,
            )?;
            if release_amount.is_zero() {
                continue;
            }
            response = response
                .add_submessages(release_msgs)
                .add_attribute(release_amount_event_key, release_amount.to_string());
            remaining_withdraw_amount = remaining_withdraw_amount.checked_sub(release_amount)?;
            release_initiated_amount = release_initiated_amount.checked_add(release_amount)?;
        }
        index += 1;
    }
    ensure!(
        transferred_amount
            .checked_add(release_initiated_amount)?
            .checked_add(remaining_withdraw_amount)?
            == amount,
        ContractError::new("Amount mismatch after transfer calculations")
    );
    Ok(response
        .add_attribute("amount", amount.to_string())
        .add_attribute("method", "release_escrow_initiate")
        .add_attribute("token", token.to_string())
        .add_attribute("release_expected", amount.to_string())
        .add_attribute("release_initiated", release_initiated_amount.to_string())
        .add_attribute("transferred_amount", transferred_amount.to_string())
        .add_attribute("unreleased_amount", remaining_withdraw_amount.to_string()))
}

pub fn _transfer_voucher_as_voucher(
    virtual_balance_address: String,
    sender: CrossChainUser,
    token: Token,
    amount: Uint128,
    recipient: Recipient,
) -> Result<(Vec<SubMsg>, Uint128), ContractError> {
    recipient.validate()?;
    ensure!(
        recipient.denom.is_voucher(),
        ContractError::new("Recipient denom is not a voucher")
    );

    let amount = match recipient.amount {
        Limit::LessThanOrEqual(limit) => amount.min(limit),
        Limit::Equal(limit) => amount.min(limit),
        Limit::GreaterThanOrEqual(limit) => {
            ensure!(
                amount.ge(&limit),
                ContractError::InsufficientAmount {
                    min_amount: limit,
                    amount
                }
            );
            amount
        }
        Limit::Dynamic(_) => amount,
    };
    if amount.is_zero() {
        return Ok((vec![], Uint128::zero()));
    }
    let forwarding_msg = match recipient.forwarding_message {
        Some(msg) => Some(Binary::from_base64(msg.as_str())?),
        None => None,
    };
    // If sender is the same as recipient, we don't need to transfer the voucher but return the amount that would have been transferred if it was different so next recipient will be calculated accordingly.
    if sender == recipient.recipient {
        return Ok((vec![], amount));
    }
    let transfer_voucher_msg = euclid::msgs::virtual_balance::msg::ExecuteMsg::Transfer(
        euclid::msgs::virtual_balance::msg::ExecuteTransfer {
            amount: amount.into(),
            token_id: token.to_string(),
            sender: Some(sender.clone()),
            to: recipient.recipient.clone(),
            from: None,
            msg: forwarding_msg,
        },
    );

    let transfer_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address,
        msg: to_json_binary(&transfer_voucher_msg)?,
        funds: vec![],
    };
    Ok((vec![SubMsg::new(transfer_voucher_msg)], amount))
}

#[allow(clippy::too_many_arguments)]
pub fn _release_voucher(
    deps: &mut DepsMut,
    env: &Env,
    virtual_balance_address: String,
    available_denoms: Vec<TokenDenom>,
    sender: CrossChainUser,
    token: Token,
    amount: Uint128,
    recipient: Recipient,
    ack_response: Option<Binary>,
    timeout: Option<u64>,
    tx_id: String,
) -> Result<(Vec<SubMsg>, Uint128), ContractError> {
    recipient.validate()?;
    ensure!(
        !recipient.denom.is_voucher(),
        ContractError::new("Recipient denom should not be a voucher")
    );
    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, recipient.recipient.chain_uid.clone())?;
    let locked_chains = LOCKED_CHAINS.load(deps.storage)?;
    ensure!(
        !locked_chains.contains(&recipient.recipient.chain_uid),
        ContractError::new("Chain is locked")
    );
    // Ensure that the preferred denom is valid
    ensure!(
        available_denoms
            .iter()
            .any(|x| x.token_type == recipient.denom.clone()
                && x.chain_uid == recipient.recipient.chain_uid),
        ContractError::InvalidDenom {}
    );

    // Query escrow balance from virtual_balance contract
    let escrow_balance_res: GetEscrowBalanceResponse = deps.querier.query_wasm_smart(
        virtual_balance_address.clone(),
        &euclid::msgs::virtual_balance::msg::QueryMsg::GetEscrowBalance {
            token_id: token.to_string(),
            chain_uid: recipient.recipient.chain_uid.clone(),
            token_type: recipient.denom.clone(),
        },
    )?;
    let escrow_balance: Uint128 = escrow_balance_res
        .balance
        .try_into()
        .unwrap_or(Uint128::MAX);

    // We cannot release more than escrow balance
    let max_release_amount = amount.min(escrow_balance);

    let release_amount = match recipient.amount {
        Limit::LessThanOrEqual(limit) => max_release_amount.min(limit),
        Limit::Equal(limit) => max_release_amount.min(limit),
        Limit::GreaterThanOrEqual(limit) => {
            ensure!(
                max_release_amount.ge(&limit),
                ContractError::InsufficientAmount {
                    min_amount: limit,
                    amount
                }
            );
            max_release_amount
        }
        Limit::Dynamic(_) => {
            ensure!(
                max_release_amount.ge(&amount),
                ContractError::InsufficientAmount {
                    min_amount: amount,
                    amount: max_release_amount
                }
            );
            amount
        }
    };
    if release_amount.is_zero() {
        return Ok((vec![], Uint128::zero()));
    }

    let release_fee_amount = get_release_fee_storage(deps, &token, &recipient.recipient.chain_uid);
    let release_amount_after_fee = release_amount.checked_sub(release_fee_amount)?;

    PENDING_RELEASE_VOUCHER.save(
        deps.storage,
        tx_id.clone(),
        &PendingReleaseVoucher {
            total_amount: release_amount,
            release_fee_amount,
            unsafe_refund_voucher: recipient.unsafe_refund_as_voucher.unwrap_or(false),
        },
    )?;
    // Prepare IBC Release Message
    let release_ibc_msg = FactoryCrossChainExecuteMsg::ReleaseEscrow {
        sender: sender.clone(),
        amount: release_amount_after_fee,
        recipient: recipient.recipient.address.clone(),
        denom: recipient.denom.clone(),
        forwarding_message: recipient.forwarding_message.clone(),
        token: token.clone(),
        tx_id,
    }
    .to_msg(
        deps,
        env,
        sender.address.clone(),
        chain,
        timeout,
        ack_response,
    )?;

    let burn_voucher_msg = euclid::msgs::virtual_balance::msg::ExecuteMsg::Burn(ExecuteBurn {
        amount: release_amount.into(),
        balance_key: BalanceKey {
            cross_chain_user: sender.clone(),
            token_id: token.to_string(),
        },
        token_type: recipient.denom.clone(),
        token_source_chain_uid: recipient.recipient.chain_uid.clone(),
    });
    let burn_voucher_msg = WasmMsg::Execute {
        contract_addr: virtual_balance_address.clone(),
        msg: to_json_binary(&burn_voucher_msg)?,
        funds: vec![],
    };

    // Escrow balance is decremented by virtual_balance during burn

    // Order matters here because we want to burn the vouchers before releasing to prevent any reentrancy attacks.
    Ok((
        vec![SubMsg::new(burn_voucher_msg), release_ibc_msg],
        release_amount,
    ))
}
