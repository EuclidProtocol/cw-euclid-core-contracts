use cosmwasm_std::{
    ensure, from_json, Addr, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128,
};

use cw20::Cw20ReceiveMsg;
use euclid::{
    cw20::Cw20HookMsg,
    error::ContractError,
    token::{Token, TokenType},
};

use crate::state::{State, ALLOWED_DENOMS, DENOM_TO_AMOUNT, STATE};

pub fn execute_add_allowed_denom(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    denom: TokenType,
) -> Result<Response, ContractError> {
    // TODO nonpayable to this function? would be better to limit depositing funds through the deposit functions
    // Only the factory can call this function
    let factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );

    let mut allowed_denoms = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();

    // Make sure that the denom isn't already in the list
    ensure!(
        !allowed_denoms.contains(&denom),
        ContractError::DuplicateDenominations {}
    );
    allowed_denoms.push(denom.clone());

    ALLOWED_DENOMS.save(deps.storage, &allowed_denoms)?;

    // Add the new denom to denom to amount map
    let new_amount =
        DENOM_TO_AMOUNT.update(deps.storage, denom.get_key(), |existing| match existing {
            Some(existing) => Ok::<_, ContractError>(existing),
            None => Ok(Uint128::zero()),
        })?;

    Ok(Response::new()
        .add_attribute("method", "add_allowed_denom")
        .add_attribute("new_denom", denom.get_key())
        .add_attribute("amount", new_amount))
}

pub fn execute_disallow_denom(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    denom: TokenType,
) -> Result<Response, ContractError> {
    // Only the factory can call this function
    let factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );

    let mut allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    // Make sure that the denom is already in the list
    ensure!(
        allowed_denoms.contains(&denom),
        ContractError::DenomDoesNotExist {}
    );
    // Remove denom from list
    allowed_denoms.retain(|current_denom| current_denom != &denom);
    ALLOWED_DENOMS.save(deps.storage, &allowed_denoms)?;

    //TODO refund the disallowed funds
    Ok(Response::new()
        .add_attribute("method", "disallow_denom")
        .add_attribute("deregistered_denom", denom.get_key()))
}

pub fn execute_update_state(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token_id: Token,
    factory_address: Addr,
    total_amount: Uint128,
) -> Result<Response, ContractError> {
    // Only the factory can call this function
    let old_factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == old_factory_address,
        ContractError::Unauthorized {}
    );

    let state = State {
        token_id: token_id.clone(),
        factory_address: factory_address.clone(),
        total_amount,
    };

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "update_state")
        .add_attribute("token_id", token_id.as_str())
        .add_attribute("factory_address", factory_address)
        .add_attribute("total_amount", total_amount))
}

pub fn execute_deposit_native(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // Make sure funds were sent
    ensure!(
        !info.funds.is_empty(),
        ContractError::InsufficientDeposit {}
    );

    // Only the factory can call this function
    let mut state = STATE.load(deps.storage)?;

    ensure!(
        info.sender == state.factory_address,
        ContractError::Unauthorized {}
    );

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    for token in info.funds {
        // Check that the amount of token sent is not zero
        ensure!(
            !token.amount.is_zero(),
            ContractError::InsufficientDeposit {}
        );
        let token_type = TokenType::Native {
            denom: token.denom.clone(),
        };
        // Make sure token is part of allowed denoms
        ensure!(
            allowed_denoms.contains(&token_type),
            ContractError::UnsupportedDenomination {}
        );

        // Check current balance of denom
        let current_balance = DENOM_TO_AMOUNT.load(deps.storage, token_type.get_key())?;

        // Add the sent amount to current balance and save it
        DENOM_TO_AMOUNT.save(
            deps.storage,
            token_type.get_key(),
            &current_balance.checked_add(token.amount)?,
        )?;
        state.total_amount = state.total_amount.checked_add(token.amount)?;
    }

    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attribute("method", "deposit"))
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::Deposit {} => {
            let factory_address = STATE.load(deps.storage)?.factory_address;
            // Only the factory can call this function
            let sender = cw20_msg.sender;
            ensure!(sender == factory_address, ContractError::Unauthorized {});

            let amount_sent = cw20_msg.amount;
            // TODO should this check be on the factory level? Or even before the factory
            ensure!(
                !amount_sent.is_zero(),
                ContractError::InsufficientDeposit {}
            );
            let asset_sent = info.sender.clone().into_string();
            let asset_sent = TokenType::Smart {
                contract_address: asset_sent,
            };

            execute_deposit_cw20(deps, env, info, amount_sent, asset_sent)
        }
        _ => Err(ContractError::UnsupportedMessage {}),
    }
}

pub fn execute_deposit_cw20(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    amount: Uint128,
    denom: TokenType,
) -> Result<Response, ContractError> {
    // Non-zero and unauthorized checks were made in receive_cw20

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    // Make sure token is part of allowed denoms
    ensure!(
        allowed_denoms.contains(&denom),
        ContractError::UnsupportedDenomination {}
    );

    // Check current balance of denom
    let current_balance = DENOM_TO_AMOUNT.load(deps.storage, denom.get_key())?;

    // Add the sent amount to current balance and save it
    DENOM_TO_AMOUNT.save(
        deps.storage,
        denom.get_key(),
        &current_balance.checked_add(amount)?,
    )?;

    // Only the factory can call this function
    let mut state = STATE.load(deps.storage)?;
    state.total_amount = state.total_amount.checked_add(amount)?;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "deposit_cw20")
        .add_attribute("asset", denom.get_key())
        .add_attribute("amount", amount))
}

pub fn execute_withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    // Only the factory can call this function
    let mut state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == state.factory_address,
        ContractError::Unauthorized {}
    );
    // Ensure that the amount desired is above zero
    ensure!(!amount.is_zero(), ContractError::ZeroWithdrawalAmount {});

    let mut messages: Vec<CosmosMsg> = Vec::new();
    let mut allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?.into_iter().peekable();

    let mut remaining_withdraw_amount = amount;
    // Ensure that the amount desired doesn't exceed the current balance
    while !remaining_withdraw_amount.is_zero() && allowed_denoms.peek().is_some() {
        let denom = allowed_denoms
            .next()
            .ok_or(ContractError::new("Denom Iter Faiiled"))?;

        let denom_balance = DENOM_TO_AMOUNT.load(deps.storage, denom.get_key())?;

        let transfer_amount = if remaining_withdraw_amount.ge(&denom_balance) {
            denom_balance
        } else {
            remaining_withdraw_amount
        };

        let send_msg = denom.create_transfer_msg(transfer_amount, recipient.to_string(), None)?;
        messages.push(send_msg);
        remaining_withdraw_amount = remaining_withdraw_amount.checked_sub(transfer_amount)?;

        DENOM_TO_AMOUNT.save(
            deps.storage,
            denom.get_key(),
            &denom_balance.checked_sub(transfer_amount)?,
        )?;
    }

    // After all the transfer messages, ensure that total amount that needs to be sent is zero
    ensure!(
        remaining_withdraw_amount.is_zero(),
        ContractError::InsufficientDeposit {}
    );

    state.total_amount = state.total_amount.checked_sub(amount)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("method", "withdraw")
        .add_attribute("amount", amount)
        .add_attribute("recipient", recipient))
}
