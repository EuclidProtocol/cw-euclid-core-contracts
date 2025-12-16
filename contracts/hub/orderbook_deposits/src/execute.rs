use cosmwasm_std::{attr, ensure, from_json, DepsMut, Env, MessageInfo, Response, StdError};
use euclid::{chain::ChainUid, msgs::hook::VirtualBalanceReceive, token::Token};

use crate::{
    error::ContractError,
    msg::{ExecuteMsg, VirtualBalanceReceiveHookMsg},
    state::{ASSET_DEPOSITS, STATE, USER_DEPOSITS, WHITELISTED_ASSETS},
};

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SetWhitelist {
            token_id,
            whitelisted,
        } => execute_set_whitelist(deps, info, token_id, whitelisted),
        ExecuteMsg::VirtualBalanceReceive(msg) => {
            execute_virtual_balance_receive(deps, env, info, msg)
        }
    }
}

fn execute_virtual_balance_receive(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    transfer: VirtualBalanceReceive,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(
        info.sender == state.virtual_balance,
        ContractError::Unauthorized {}
    );
    ensure!(
        matches!(state.status, crate::state::OrderbookDepositsStatus::Active),
        ContractError::Unauthorized {}
    );

    let hook: VirtualBalanceReceiveHookMsg = from_json(transfer.msg.clone())?;
    match hook {
        VirtualBalanceReceiveHookMsg::Deposit {} => {
            execute_deposit(deps, transfer.token_id, transfer.amount, transfer.sender)
        }
    }
}

fn execute_deposit(
    deps: DepsMut,
    token_id: String,
    amount: cosmwasm_std::Uint128,
    sender: euclid::chain::CrossChainUser,
) -> Result<Response, ContractError> {
    ensure!(!amount.is_zero(), ContractError::InvalidAmount {});

    let is_whitelisted = WHITELISTED_ASSETS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or(false);
    ensure!(is_whitelisted, ContractError::AssetNotWhitelisted {});

    let _ = Token::create(token_id.clone())?;
    let chain_uid = ChainUid::vsl_chain_uid()?;
    ensure!(
        sender.chain_uid == chain_uid,
        ContractError::Unauthorized {}
    );
    let sender_addr = deps.api.addr_validate(&sender.address)?;

    // Update aggregate and user-level deposit tracking.
    let new_asset_total = ASSET_DEPOSITS
        .may_load(deps.storage, token_id.clone())?
        .unwrap_or_default()
        .checked_add(amount)
        .map_err(StdError::from)?;
    ASSET_DEPOSITS.save(deps.storage, token_id.clone(), &new_asset_total)?;

    let user_key = (sender_addr.clone(), token_id.clone());
    let new_user_total = USER_DEPOSITS
        .may_load(deps.storage, user_key.clone())?
        .unwrap_or_default()
        .checked_add(amount)
        .map_err(StdError::from)?;
    USER_DEPOSITS.save(deps.storage, user_key, &new_user_total)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "deposit"),
        attr("token_id", token_id),
        attr("amount", amount.to_string()),
        attr("sender", sender_addr.as_str()),
    ]))
}

fn execute_set_whitelist(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,
    whitelisted: bool,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let token = Token::create(token_id.clone())?;
    WHITELISTED_ASSETS.save(deps.storage, token.to_string(), &whitelisted)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "set_whitelist"),
        attr("token_id", token_id),
        attr("whitelisted", whitelisted.to_string()),
    ]))
}
