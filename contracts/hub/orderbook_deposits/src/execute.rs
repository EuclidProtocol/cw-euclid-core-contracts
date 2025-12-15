use cosmwasm_std::{attr, ensure, DepsMut, Env, MessageInfo, Response, StdError, Uint128};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    token::Token,
};

use crate::{
    error::ContractError,
    msg::ExecuteMsg,
    state::{ASSET_DEPOSITS, STATE, USER_DEPOSITS, WHITELISTED_ASSETS},
};

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Deposit { token_id, amount } => {
            execute_deposit(deps, env, info, token_id, amount)
        }
    }
}

fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    ensure!(!amount.is_zero(), ContractError::InvalidAmount {});

    let state = STATE.load(deps.storage)?;

    let is_whitelisted = WHITELISTED_ASSETS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or(false);
    ensure!(is_whitelisted, ContractError::AssetNotWhitelisted {});

    let token = Token::create(token_id.clone())?;
    let chain_uid = ChainUid::vsl_chain_uid()?;

    // Transfer vouchers from the sender to this contract's virtual balance account.
    let transfer_msg = token.create_virtual_balance_transfer_msg(
        state.virtual_balance.to_string(),
        amount,
        None,
        CrossChainUser {
            chain_uid: chain_uid.clone(),
            address: env.contract.address.to_string(),
        },
        Some(CrossChainUser {
            chain_uid,
            address: info.sender.to_string(),
        }),
        None,
    )?;

    // Update aggregate and user-level deposit tracking.
    let new_asset_total = ASSET_DEPOSITS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or_default()
        .checked_add(amount)
        .map_err(StdError::from)?;
    ASSET_DEPOSITS.save(deps.storage, token_id.clone(), &new_asset_total)?;

    let user_key = (info.sender.clone(), token_id.clone());
    let new_user_total = USER_DEPOSITS
        .may_load(deps.storage, user_key.clone())?
        .unwrap_or_default()
        .checked_add(amount)
        .map_err(StdError::from)?;
    USER_DEPOSITS.save(deps.storage, user_key, &new_user_total)?;

    Ok(Response::new()
        .add_message(transfer_msg)
        .add_attributes(vec![
            attr("action", "deposit"),
            attr("token_id", token_id),
            attr("amount", amount.to_string()),
            attr("sender", info.sender.as_str()),
        ]))
}
