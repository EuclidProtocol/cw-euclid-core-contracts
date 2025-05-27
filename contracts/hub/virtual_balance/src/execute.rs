use cosmwasm_std::{ensure, Addr, Attribute, DepsMut, MessageInfo, Response, Uint128, WasmMsg};
use euclid::{
    chain::{ChainUid, CrossChainUser},
    error::ContractError,
    msgs::{
        hook::VirtualBalanceReceive,
        virtual_balance::{ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteTransfer, State},
    },
    virtual_balance::BalanceKey,
};

use crate::state::{Allowance, ALLOWANCES, BALANCES, STATE};

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

    let mut response = Response::new()
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
        .add_attribute("mint_token_id", msg.balance_key.token_id.clone())
        .add_attribute("new_balance", new_balance);

    // If there was a forward message, then we need to send it to the receiver
    if let Some(forward_msg) = msg.forward_msg {
        let virtual_balance_receive = VirtualBalanceReceive {
            sender: msg.balance_key.cross_chain_user.clone(),
            amount: msg.amount,
            token_id: msg.balance_key.token_id.clone(),
            msg: forward_msg,
        };
        let execute_msg = WasmMsg::Execute {
            contract_addr: msg.balance_key.cross_chain_user.address,
            msg: virtual_balance_receive.to_receiver_msg()?,
            funds: vec![],
        };
        response = response.add_message(execute_msg);
    }
    Ok(response)
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
    deps: &mut DepsMut,
    info: MessageInfo,
    msg: ExecuteTransfer,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let sender = if let Some(sender) = msg.sender {
        // Only  router can set pseudo sender
        ensure!(info.sender == state.router, ContractError::Unauthorized {});
        sender
    } else {
        CrossChainUser::new(ChainUid::vsl_chain_uid()?, info.sender.to_string())
    };

    let mut response = if let Some(from) = msg.from {
        let attributes = _deduct_allowance(deps, &sender, &from, msg.amount, &msg.token_id)?;
        let transfer_response =
            _transfer(deps, from, msg.to.clone(), msg.amount, msg.token_id.clone())?;
        transfer_response.add_attributes(attributes)
    } else {
        _transfer(
            deps,
            sender.clone(),
            msg.to.clone(),
            msg.amount,
            msg.token_id.clone(),
        )?
    };

    if let Some(forward_msg) = msg.msg {
        let forward_msg = VirtualBalanceReceive {
            sender,
            amount: msg.amount,
            token_id: msg.token_id.clone(),
            msg: forward_msg,
        };
        // Send message to receiver. Caution should be take to not trigger a message with wrong chain uid as chain uid is not verified here.
        let euclid_receive = WasmMsg::Execute {
            contract_addr: msg.to.address,
            msg: forward_msg.to_receiver_msg()?,
            funds: vec![],
        };
        response = response.add_message(euclid_receive);
    }
    Ok(response)
}

fn _transfer(
    deps: &mut DepsMut,
    from: CrossChainUser,
    to: CrossChainUser,
    amount: Uint128,
    token_id: String,
) -> Result<Response, ContractError> {
    ensure!(!amount.is_zero(), ContractError::ZeroAssetAmount {});
    ensure!(to != from, ContractError::SameAddress {});

    let sender_balance_key = BalanceKey {
        token_id: token_id.clone(),
        cross_chain_user: from.clone(),
    };

    let sender_key = sender_balance_key.clone().to_serialized_balance_key();

    // Decrease sender balance
    let sender_old_balance = BALANCES
        .may_load(deps.storage, sender_key.clone())?
        .unwrap_or(Uint128::zero());

    // This might not be needed because checked sub will do this check anyways.
    // Added here just for additional safety
    ensure!(
        sender_old_balance.ge(&amount),
        ContractError::new(&format!(
            "Not Enough Funds: {} < {}",
            sender_old_balance, amount
        ))
    );

    let sender_new_balance = sender_old_balance.checked_sub(amount)?;
    BALANCES.save(deps.storage, sender_key, &sender_new_balance)?;

    let receiver_balance_key = BalanceKey {
        token_id: token_id.clone(),
        cross_chain_user: to.clone(),
    };
    let receiver_key = receiver_balance_key.clone().to_serialized_balance_key();

    // Increase receiver balance
    let receiver_old_balance = BALANCES
        .may_load(deps.storage, receiver_key.clone())?
        .unwrap_or(Uint128::zero());
    let receiver_new_balance = receiver_old_balance.checked_add(amount)?;
    BALANCES.save(deps.storage, receiver_key, &receiver_new_balance)?;

    let response = Response::new()
        .add_attribute("action", "execute_transfer")
        .add_attribute("transfer_amount", amount)
        .add_attribute("from", format!("{sender_balance_key:?}"))
        .add_attribute("to", format!("{receiver_balance_key:?}"))
        .add_attribute("token_id", token_id);

    Ok(response)
}

fn _deduct_allowance(
    deps: &mut DepsMut,
    sender: &CrossChainUser,
    from: &CrossChainUser,
    amount: Uint128,
    token_id: &str,
) -> Result<Vec<Attribute>, ContractError> {
    let sender_balance_key = BalanceKey {
        token_id: token_id.to_string(),
        cross_chain_user: from.clone(),
    };
    let mut allowance = ALLOWANCES
        .load(
            deps.storage,
            sender_balance_key.clone().to_serialized_balance_key(),
        )
        .unwrap_or(Allowance {
            amount: Uint128::zero(),
            spender: from.clone(),
        });
    ensure!(
        allowance.spender == sender.clone(),
        ContractError::Unauthorized {}
    );

    ensure!(
        allowance.amount.ge(&amount),
        ContractError::new(&format!(
            "Not Enough Allowance: {} < {}",
            allowance.amount, amount
        ))
    );

    allowance.amount = allowance.amount.checked_sub(amount)?;
    ALLOWANCES.save(
        deps.storage,
        sender_balance_key.to_serialized_balance_key(),
        &allowance,
    )?;

    Ok(vec![
        Attribute::new("allowance_used", amount),
        Attribute::new("new_allowance", allowance.amount),
    ])
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

    let spender = msg.spender;

    // If router is approving, then owner is the owner
    // If user is approving, then owner is the user from info.sender and vsl chain
    let owner = if info.sender == state.router {
        msg.owner
    } else {
        CrossChainUser::new(vsl_chain_uid, info.sender.to_string())
    };

    // Ensure that spender and owner are not the same
    ensure!(spender != owner, ContractError::SameAddress {});

    let key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: owner.clone(),
    };
    ALLOWANCES.save(
        deps.storage,
        key.to_serialized_balance_key(),
        &Allowance {
            amount: msg.amount,
            spender: spender.clone(),
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "execute_approve")
        .add_attribute("approve_amount", msg.amount)
        .add_attribute("approve_token_id", msg.token_id)
        .add_attribute("approve_spender", spender.to_sender_string())
        .add_attribute("approve_owner", owner.to_sender_string()))
}
