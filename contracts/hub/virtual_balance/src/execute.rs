use cosmwasm_std::{ensure, Addr, DepsMut, MessageInfo, Response, Uint128};
use euclid::{
    chain::ChainUid,
    error::ContractError,
    msgs::virtual_balance::{
        Allowance, ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteTransfer, State,
        VBalanceMigrateMsg,
    },
    virtual_balance::BalanceKey,
};

use crate::state::{ALLOWANCES, BALANCES, STATE};

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

    ensure!(!msg.amount.is_zero(), ContractError::ZeroAssetAmount {});

    let sender_balance_key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: msg.from.clone(),
    };

    let allowance = ALLOWANCES
        .load(
            deps.storage,
            sender_balance_key.clone().to_serialized_balance_key(),
        )
        .unwrap_or(Allowance {
            amount: Uint128::zero(),
            spender: msg.from.clone(),
        });

    // Router can send on behalf of anyone, or any user can transfer his own funds, or if allowance is set and greater than amount
    ensure!(
        state.router == info.sender
            || (allowance.spender.address == info.sender && allowance.amount.ge(&msg.amount))
            || (sender_balance_key.cross_chain_user.address == info.sender
                && sender_balance_key.cross_chain_user.chain_uid == ChainUid::vsl_chain_uid()?),
        ContractError::Unauthorized {}
    );

    // Make sure the sender and recipient are not the same
    ensure!(
        msg.to != sender_balance_key.cross_chain_user,
        ContractError::SameAddress {}
    );

    let sender_key = sender_balance_key.clone().to_serialized_balance_key();

    // Decrease sender balance
    let sender_old_balance = BALANCES
        .may_load(deps.storage, sender_key.clone())?
        .unwrap_or(Uint128::zero());

    // This might not be needed because checked sub will do this check anyways.
    // Added here just for additional safety
    ensure!(
        sender_old_balance.ge(&msg.amount),
        ContractError::new(&format!(
            "Not Enough Funds: {} < {}",
            sender_old_balance, msg.amount
        ))
    );

    let sender_new_balance = sender_old_balance.checked_sub(msg.amount)?;
    BALANCES.save(deps.storage, sender_key, &sender_new_balance)?;

    let receiver_balance_key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: msg.to,
    };
    let receiver_key = receiver_balance_key.clone().to_serialized_balance_key();

    // Increase receiver balance
    let receiver_old_balance = BALANCES
        .may_load(deps.storage, receiver_key.clone())?
        .unwrap_or(Uint128::zero());
    let receiver_new_balance = receiver_old_balance.checked_add(msg.amount)?;
    BALANCES.save(deps.storage, receiver_key, &receiver_new_balance)?;

    let mut response = Response::new()
        .add_attribute("action", "execute_transfer")
        .add_attribute("transfer_amount", msg.amount)
        .add_attribute("from", format!("{sender_balance_key:?}"))
        .add_attribute("to", format!("{receiver_balance_key:?}"))
        .add_attribute("token_id", msg.token_id);

    if allowance.amount.ge(&msg.amount) {
        let mut new_allowance = allowance;
        new_allowance.amount = new_allowance.amount.checked_sub(msg.amount)?;
        ALLOWANCES.save(
            deps.storage,
            sender_balance_key.clone().to_serialized_balance_key(),
            &new_allowance,
        )?;
        response = response
            .add_attribute("allowance_used", msg.amount)
            .add_attribute("new_allowance", new_allowance.amount);
    }

    Ok(response)
}

pub fn execute_update_state(
    deps: DepsMut,
    info: MessageInfo,
    router: Option<String>,
    admin: Option<Addr>,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    let verified_router = if let Some(ref router) = router {
        deps.api.addr_validate(router.as_str())?;
        router.clone()
    } else {
        state.router
    };

    let verified_admin = if let Some(ref admin) = admin {
        deps.api.addr_validate(admin.as_str())?;
        admin.clone()
    } else {
        state.admin
    };

    let new_state = State {
        router: verified_router,
        admin: verified_admin,
    };

    STATE.save(deps.storage, &new_state)?;

    Ok(Response::new()
        .add_attribute("action", "execute_update_state")
        .add_attribute(
            "router",
            router.map_or_else(|| "unchanged".to_string(), |router| router.to_string()),
        )
        .add_attribute(
            "admin",
            admin.map_or_else(|| "unchanged".to_string(), |admin| admin.to_string()),
        ))
}

pub fn execute_approve(
    deps: DepsMut,
    info: MessageInfo,
    msg: ExecuteApprove,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let vsl_chain_uid = ChainUid::vsl_chain_uid()?;

    // Ensure that spender is on vsl chain
    ensure!(
        msg.spender.chain_uid == vsl_chain_uid,
        ContractError::Unauthorized {}
    );

    // Ensure that spender and owner are not the same
    ensure!(msg.spender != msg.owner, ContractError::SameAddress {});

    // Router can send on behalf of anyone, or any user can transfer his own funds
    ensure!(
        state.router == info.sender
            || (msg.owner.address == info.sender && msg.owner.chain_uid == vsl_chain_uid),
        ContractError::Unauthorized {}
    );

    let key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: msg.owner.clone(),
    };
    ALLOWANCES.save(
        deps.storage,
        key.to_serialized_balance_key(),
        &Allowance {
            amount: msg.amount,
            spender: msg.spender.clone(),
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "execute_approve")
        .add_attribute("approve_amount", msg.amount)
        .add_attribute("approve_token_id", msg.token_id)
        .add_attribute("approve_spender", msg.spender.to_sender_string())
        .add_attribute("approve_owner", msg.owner.to_sender_string()))
}

pub fn execute_migrate_vbalance(
    deps: DepsMut,
    info: MessageInfo,
    msg: VBalanceMigrateMsg,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.admin, ContractError::Unauthorized {});

    STATE.save(deps.storage, &msg.state.state)?;
    for (key, value) in msg.balances.balances {
        BALANCES.save(deps.storage, key, &value)?;
    }
    for (key, value) in msg.allowances.allowances {
        ALLOWANCES.save(deps.storage, key, &value)?;
    }
    Ok(Response::new().add_attribute("action", "execute_migrate_vbalance"))
}
