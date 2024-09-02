use cosmwasm_std::{ensure, Addr, DepsMut, MessageInfo, Response, Uint128};
use euclid::{
    chain::ChainUid,
    error::ContractError,
    msgs::virtual_balance::{ExecuteBurn, ExecuteMint, ExecuteTransfer, State},
    virtual_balance::BalanceKey,
};

use crate::state::{BALANCES, STATE};

pub fn execute_mint(
    deps: DepsMut,
    info: MessageInfo,
    msg: ExecuteMint,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    // Zero amounts not allowed
    ensure!(!msg.amount.is_zero(), ContractError::ZeroAssetAmount {});

    let key = msg.balance_key.clone().to_serialized_balance_key();

    let old_balance = BALANCES
        .may_load(deps.storage, key.clone())?
        .unwrap_or(Uint128::zero());

    let new_balance = old_balance.checked_add(msg.amount)?;

    BALANCES.save(deps.storage, key, &new_balance)?;

    Ok(Response::new()
        .add_attribute("action", "execute_mint")
        .add_attribute("mint_amount", msg.amount)
        .add_attribute(
            "mint_address",
            msg.balance_key.cross_chain_user.to_sender_string(),
        )
        .add_attribute(
            "mint_address_chain",
            msg.balance_key.cross_chain_user.chain_uid.to_string(),
        )
        .add_attribute("mint_token_id", msg.balance_key.token_id)
        .add_attribute("new_balance", new_balance))
}

pub fn execute_burn(
    deps: DepsMut,
    info: MessageInfo,
    msg: ExecuteBurn,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    // Zero amounts not allowed
    ensure!(!msg.amount.is_zero(), ContractError::ZeroAssetAmount {});

    let key = msg.balance_key.clone().to_serialized_balance_key();

    let old_balance = BALANCES
        .may_load(deps.storage, key.clone())?
        .unwrap_or(Uint128::zero());

    let new_balance = old_balance.checked_sub(msg.amount)?;

    BALANCES.save(deps.storage, key, &new_balance)?;

    Ok(Response::new()
        .add_attribute("action", "execute_burn")
        .add_attribute("burn_amount", msg.amount)
        .add_attribute(
            "burn_address",
            msg.balance_key.cross_chain_user.to_sender_string(),
        )
        .add_attribute(
            "burn_address_chain",
            msg.balance_key.cross_chain_user.chain_uid.to_string(),
        )
        .add_attribute("burn_token_id", msg.balance_key.token_id)
        .add_attribute("new_balance", new_balance))
}

pub fn execute_transfer(
    deps: DepsMut,
    info: MessageInfo,
    msg: ExecuteTransfer,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    // Router can send on behalf of anyone, or any user can transfer his own funds
    ensure!(
        state.router == info.sender
            || (msg.from.address == info.sender
                && msg.from.chain_uid == ChainUid::vsl_chain_uid()?),
        ContractError::Unauthorized {}
    );

    let sender_balance_key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: msg.from,
    };
    let sender_key = sender_balance_key.clone().to_serialized_balance_key();

    let receiver_balance_key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: msg.to,
    };
    let receiver_key = receiver_balance_key.clone().to_serialized_balance_key();

    // Decrease sender balance
    let sender_old_balance = BALANCES
        .may_load(deps.storage, sender_key.clone())?
        .unwrap_or(Uint128::zero());

    // This might not be needed because checked sub will do this check anyways.
    // Added here just for additional safety
    ensure!(
        sender_old_balance.ge(&msg.amount),
        ContractError::Generic {
            err: "Not Enough Funds".to_string()
        }
    );

    let sender_new_balance = sender_old_balance.checked_sub(msg.amount)?;

    // Increase receiver balance
    let receiver_old_balance = BALANCES
        .may_load(deps.storage, receiver_key.clone())?
        .unwrap_or(Uint128::zero());
    let receiver_new_balance = receiver_old_balance.checked_add(msg.amount)?;

    BALANCES.save(deps.storage, sender_key, &sender_new_balance)?;

    BALANCES.save(deps.storage, receiver_key, &receiver_new_balance)?;

    Ok(Response::new()
        .add_attribute("action", "execute_transfer")
        .add_attribute("transfer_amount", msg.amount)
        .add_attribute("from", format!("{sender_balance_key:?}"))
        .add_attribute("to", format!("{receiver_balance_key:?}"))
        .add_attribute("burn_token_id", msg.token_id))
}

pub fn execute_update_state(
    deps: DepsMut,
    info: MessageInfo,
    router: String,
    admin: Addr,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let new_state = State {
        router: router.clone(),
        admin: admin.clone(),
    };
    STATE.save(deps.storage, &new_state)?;

    Ok(Response::new()
        .add_attribute("action", "execute_update_state")
        .add_attribute("router", router)
        .add_attribute("admin", admin.to_string()))
}
