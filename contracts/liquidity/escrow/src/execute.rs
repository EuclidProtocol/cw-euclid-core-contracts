use std::collections::HashSet;

use cosmwasm_std::{
    ensure, from_json, to_json_binary, BankMsg, Coin, CosmosMsg, DepsMut, Env, MessageInfo,
    Response, StdResult, Uint128, WasmMsg,
};

use cw20::Cw20ReceiveMsg;
use euclid::{
    cw20::Cw20ExecuteMsg,
    error::ContractError,
    msgs::{escrow::AmountAndType, pool::Cw20HookMsg},
};

use crate::state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT, STATE};

fn check_duplicates(denoms: Vec<String>) -> Result<(), ContractError> {
    let mut seen = HashSet::new();
    for denom in denoms {
        if seen.contains(&denom) {
            return Err(ContractError::DuplicateDenominations {});
        }
        seen.insert(denom);
    }
    Ok(())
}

// Function to add a new list of allowed denoms, this overwrites the previous list
pub fn execute_add_allowed_denom(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
) -> Result<Response, ContractError> {
    // TODO nonpayable to this function? would be better to limit depositing funds through the deposit functions
    // Only the factory can call this function
    let factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );

    let mut allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    // Make sure that the denom isn't already in the list
    ensure!(
        !allowed_denoms.contains(&denom),
        ContractError::DuplicateDenominations {}
    );
    allowed_denoms.push(denom);

    ALLOWED_DENOMS.save(deps.storage, &allowed_denoms)?;

    // Add the new denom to denom to amount map, and set its balance as zero
    // The is_native will be overwriting once the denom is funded
    DENOM_TO_AMOUNT.save(
        deps.storage,
        denom,
        &AmountAndType {
            amount: Uint128::zero(),
            is_native: true,
        },
    )?;

    Ok(Response::new()
        .add_attribute("method", "add_allowed_denom")
        .add_attribute("new_denom", denom))
}

pub fn execute_deposit_native(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // Only the factory can call this function
    let factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    // Make sure funds were sent
    ensure!(
        !info.funds.is_empty(),
        ContractError::InsufficientDeposit {}
    );

    for token in info.funds {
        // Check that the amount of token sent is not zero
        ensure!(
            !token.amount.is_zero(),
            ContractError::InsufficientDeposit {}
        );
        // Make sure token is part of allowed denoms
        ensure!(
            allowed_denoms.contains(&token.denom),
            ContractError::UnsupportedDenomination {}
        );

        // Check current balance of denom
        let current_balance = DENOM_TO_AMOUNT.load(deps.storage, token.denom)?;

        // Add the sent amount to current balance and save it
        DENOM_TO_AMOUNT.save(
            deps.storage,
            token.denom,
            &AmountAndType {
                amount: current_balance.amount.checked_add(token.amount)?,
                is_native: true,
            },
        );
    }

    Ok(Response::new().add_attribute("method", "deposit"))
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::Deposit {} => {
            // Only the factory can call this function
            let factory_address = STATE.load(deps.storage)?.factory_address;
            let sender = cw20_msg.sender;
            ensure!(sender == factory_address, ContractError::Unauthorized {});

            let asset_sent = info.sender.clone().into_string();
            let amount_sent = cw20_msg.amount;
            // TODO should this check be on the factory level? Or even before the factory
            ensure!(
                !amount_sent.is_zero(),
                ContractError::InsufficientDeposit {}
            );

            execute_deposit_cw20(deps, env, info, amount_sent, asset_sent)
        }
        _ => Err(ContractError::UnsupportedMessage {}),
    }
}

pub fn execute_deposit_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    denom: String,
) -> Result<Response, ContractError> {
    // Non-zero and unauthorized checks were made in receive_cw20

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    // Make sure token is part of allowed denoms
    ensure!(
        allowed_denoms.contains(&denom),
        ContractError::UnsupportedDenomination {}
    );

    // Check current balance of denom
    let current_balance = DENOM_TO_AMOUNT.load(deps.storage, denom)?;

    // Add the sent amount to current balance and save it
    DENOM_TO_AMOUNT.save(
        deps.storage,
        denom,
        &AmountAndType {
            amount: current_balance.amount.checked_add(amount)?,
            is_native: false,
        },
    );

    Ok(Response::new()
        .add_attribute("method", "deposit_cw20")
        .add_attribute("asset", denom)
        .add_attribute("amount", amount))
}

pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
    chain_id: String,
) -> Result<Response, ContractError> {
    // Only the factory can call this function
    let factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );
    // Ensure that the amount desired is above zero
    ensure!(!amount.is_zero(), ContractError::ZeroWithdrawalAmount {});

    // For now we only support local chain transfers
    ensure!(
        env.block.chain_id == chain_id,
        ContractError::UnsupportedOperation {}
    );

    // Ensure that the amount desired doesn't exceed the current balance
    let mut sum = Uint128::zero();
    let mut total_sum: Result<(), ContractError> = DENOM_TO_AMOUNT
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .try_for_each(|item| {
            let (_, value) = item?;
            sum = sum.checked_add(value.amount)?;
            Ok(())
        });
    ensure!(amount.le(&sum), ContractError::InvalidWithdrawalAmount {});

    let mut messages: Vec<CosmosMsg> = Vec::new();

    // Create a vector of (key, value) pairs
    let mut amounts: Vec<(String, AmountAndType)> = DENOM_TO_AMOUNT
        .range(deps.storage, None, None, cosmwasm_std::Order::Descending)
        .map(|item| {
            let (key, value) = item?;
            Ok((key.to_string(), value))
        })
        .collect::<StdResult<Vec<(String, AmountAndType)>>>()?;
    let mut amount_to_withdraw = amount;
    let mut amount_to_send = Uint128::zero();

    // Iterate through the amounts and deduct the required amount
    for (denom, amount) in &mut amounts {
        if amount_to_withdraw.is_zero() {
            break;
        }
        if amount.amount >= amount_to_withdraw {
            // If the current denom has enough amount to cover the withdrawal
            amount.amount -= amount_to_withdraw;
            // Send the amount to withdraw since it can cover the entire amount
            amount_to_send = amount_to_withdraw;
            amount_to_withdraw = Uint128::zero();
        } else {
            // If the current denom doesn't have enough amount
            amount_to_withdraw -= amount.amount;

            // Send the entire amount
            amount_to_send = amount.amount;

            // Update balance
            amount.amount = Uint128::zero();
            // Create message
        }
        // If denom is native
        if amount.is_native {
            messages.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient,
                amount: vec![Coin {
                    denom: denom.to_string(),
                    amount: amount_to_send,
                }],
            }));
        } else {
            // If denom is cw20
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: denom.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient.to_string(),
                    amount: amount_to_send,
                })?,
                funds: vec![],
            }))
        }
    }
    // Check if we could fulfill the entire amount_to_withdraw
    if !amount_to_withdraw.is_zero() {
        return Err(ContractError::InsufficientDeposit {});
    }

    // Update the storage with new amounts
    for (denom, amount) in amounts {
        DENOM_TO_AMOUNT.save(deps.storage, denom, &amount)?;
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("method", "withdraw")
        .add_attribute("amount", amount)
        .add_attribute("recipient", recipient))
}
