use std::collections::HashSet;

use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcMsg,
    IbcTimeout, MessageInfo, Response, Uint128,
};

use cw20::Cw20ReceiveMsg;
use euclid::{
    error::ContractError,
    liquidity,
    msgs::pool::Cw20HookMsg,
    pool::LiquidityResponse,
    swap::{self, SwapResponse},
    timeout::get_timeout,
    token::{Token, TokenInfo},
};
use euclid_ibc::msg::ChainIbcExecuteMsg;

use crate::state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT, FACTORY_ADDRESS};

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
    let factory_address = FACTORY_ADDRESS.load(deps.storage)?;
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
    DENOM_TO_AMOUNT.save(deps.storage, denom, &Uint128::zero())?;

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
    let factory_address = FACTORY_ADDRESS.load(deps.storage)?;
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
        // Make sure token is part of allowed denoms
        ensure!(
            allowed_denoms.contains(&token.denom),
            ContractError::UnsupportedDenomination {}
        );

        // Check current balance of denom
        let current_balance = DENOM_TO_AMOUNT.load(deps.storage, token.denom)?;

        // Check that the amount of token sent is not zero
        ensure!(
            !token.amount.is_zero(),
            ContractError::InsufficientDeposit {}
        );
        // Add the sent amount to current balance and save it
        DENOM_TO_AMOUNT.save(
            deps.storage,
            token.denom,
            &current_balance.checked_add(token.amount)?,
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
        // Allow to swap using a CW20 hook message
        Cw20HookMsg::Swap {
            asset,
            min_amount_out,
            timeout,
        } => {
            let contract_adr = info.sender.clone();

            // ensure that contract address is same as asset being swapped
            ensure!(
                contract_adr == asset.get_contract_address(),
                ContractError::AssetDoesNotExist {}
            );
            // Add sender as the option

            // ensure that the contract address is the same as the asset contract address
            execute_swap_request(
                &mut deps,
                info,
                env,
                asset,
                cw20_msg.amount,
                min_amount_out,
                Some(cw20_msg.sender),
                timeout,
            )
        }
    }
}
