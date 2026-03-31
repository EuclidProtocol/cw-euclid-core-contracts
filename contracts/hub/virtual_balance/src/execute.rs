use cosmwasm_std::{
    ensure, Addr, Attribute, DepsMut, Env, MessageInfo, Order, Response, Uint128, WasmMsg,
};
use cw_storage_plus::Bound;
use euclid::{
    admin::{self, AdminType},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{
        hook::VoucherReceive,
        virtual_balance::msg::{
            Allowance, ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteTransfer,
        },
    },
    voucher::{BalanceKey, SerializedBalanceKey},
};

use crate::state::{ADMIN, ALLOWANCES, BALANCES, STATE};

pub fn execute_mint(
    deps: DepsMut,
    info: MessageInfo,
    msg: ExecuteMint,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    // Only router can mint vouchers
    ensure!(info.sender == state.router, ContractError::Unauthorized {});
    // Zero amounts not allowed
    ensure!(!msg.amount.is_zero(), ContractError::ZeroAssetAmount {});
    msg.balance_key.cross_chain_user.validate()?;

    let key = msg.balance_key.clone().to_serialized_balance_key();

    let old_balance = BALANCES
        .may_load(deps.storage, key.clone())?
        .unwrap_or(Uint128::zero());

    let new_balance = old_balance.checked_add(msg.amount)?;

    BALANCES.save(deps.storage, key, &new_balance)?;

    let response = Response::new()
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

    Ok(response)
}

pub fn execute_burn(
    deps: DepsMut,
    info: MessageInfo,
    msg: ExecuteBurn,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    // Only router can burn vouchers
    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    // Zero amounts not allowed
    ensure!(!msg.amount.is_zero(), ContractError::ZeroAssetAmount {});
    msg.balance_key.cross_chain_user.validate()?;

    let key = msg.balance_key.clone().to_serialized_balance_key();
    let old_balance =
        BALANCES
            .may_load(deps.storage, key.clone())?
            .ok_or(ContractError::BalanceNotFound {
                key: format!("{:?}", key),
            })?;

    let new_balance = old_balance.checked_sub(msg.amount)?;

    if new_balance.is_zero() {
        // Lets clear some space from chain storage
        BALANCES.remove(deps.storage, key);
    } else {
        BALANCES.save(deps.storage, key, &new_balance)?;
    }

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
    transfer_msg: ExecuteTransfer,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    let sender = if let Some(sender) = transfer_msg.sender {
        ensure!(
            info.sender == state.router,
            ContractError::UnauthorizedWithMsg {
                msg: "Only router can set pseudo sender".to_string(),
            }
        );
        sender
    } else {
        CrossChainUser::new(ChainUid::vsl_chain_uid()?, info.sender.to_string())
    };
    sender.validate()?;
    transfer_msg.to.validate()?;

    let mut response = if let Some(from) = transfer_msg.from {
        from.validate()?;
        let attributes = _deduct_allowance(
            deps,
            &sender,
            &from,
            transfer_msg.amount,
            &transfer_msg.token_id,
        )?;
        let transfer_response = _transfer(
            deps,
            from,
            transfer_msg.to.clone(),
            transfer_msg.amount,
            transfer_msg.token_id.clone(),
        )?;
        transfer_response.add_attributes(attributes)
    } else {
        _transfer(
            deps,
            sender.clone(),
            transfer_msg.to.clone(),
            transfer_msg.amount,
            transfer_msg.token_id.clone(),
        )?
    };

    if let Some(forward_msg) = transfer_msg.msg {
        ensure!(
            transfer_msg.to.chain_uid == ChainUid::vsl_chain_uid()?,
            ContractError::new("Voucher transfer with message must be to vsl chain")
        );
        let forward_msg = VoucherReceive {
            sender,
            amount: transfer_msg.amount,
            token_id: transfer_msg.token_id.clone(),
            msg: forward_msg,
        };
        // Send message to receiver. Caution should be take to not trigger a message with wrong chain uid as chain uid is not verified here.
        let euclid_receive = WasmMsg::Execute {
            contract_addr: transfer_msg.to.address,
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
    if sender_new_balance.is_zero() {
        BALANCES.remove(deps.storage, sender_key);
    } else {
        BALANCES.save(deps.storage, sender_key, &sender_new_balance)?;
    }

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
    let serialized_balance_key = sender_balance_key.clone().to_serialized_balance_key();
    let mut allowance = ALLOWANCES
        .load(deps.storage, serialized_balance_key.clone())
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
    if allowance.amount.is_zero() {
        ALLOWANCES.remove(deps.storage, serialized_balance_key);
    } else {
        ALLOWANCES.save(deps.storage, serialized_balance_key, &allowance)?;
    }

    Ok(vec![
        Attribute::new("allowance_used", amount),
        Attribute::new("new_allowance", allowance.amount),
    ])
}

pub fn execute_update_admin(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_admin: String,
    admin_type: AdminType,
) -> Result<Response, ContractError> {
    let current_admin = ADMIN.load(deps.storage)?;
    let (updated_admins, response) = admin::update_admin(
        &current_admin,
        &deps,
        &env,
        &info.sender,
        new_admin.clone(),
        admin_type,
    )?;

    ADMIN.save(deps.storage, &updated_admins)?;
    Ok(response
        .add_attribute("old_admin", current_admin.to_string())
        .add_attribute("new_admin", new_admin.to_string()))
}

pub fn execute_update_router(
    deps: DepsMut,
    info: MessageInfo,
    router: Addr,
) -> Result<Response, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        info.sender == admin.general_admin,
        ContractError::Unauthorized {}
    );

    let verified_router = deps.api.addr_validate(router.as_str())?;
    let mut state = STATE.load(deps.storage)?;
    state.router = verified_router.clone();
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "execute_update_state")
        .add_attribute("router", verified_router))
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
        msg.owner.clone()
    } else {
        CrossChainUser::new(vsl_chain_uid.clone(), info.sender.to_string())
    };
    spender.validate()?;
    owner.validate()?;

    // Ensure that spender and owner are not the same
    ensure!(spender != owner, ContractError::SameAddress {});

    // Router can send on behalf of anyone, or any user can transfer his own funds
    ensure!(
        state.router == info.sender
            || (msg.owner.address == info.sender.to_string()
                && msg.owner.chain_uid == vsl_chain_uid),
        ContractError::Unauthorized {}
    );

    let key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: owner.clone(),
    };

    ensure!(!msg.amount.is_zero(), ContractError::ZeroAssetAmount {});
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

pub fn execute_remove_zero_state_values(
    deps: DepsMut,
    info: MessageInfo,
    start_after: Option<SerializedBalanceKey>,
    limit: Option<u32>,
) -> Result<Response, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        admin.general_admin == info.sender,
        ContractError::Unauthorized {}
    );

    // Remove Allowances with a value of zero
    let limit = limit.unwrap_or(u32::MAX) as usize;
    let start = start_after.map(Bound::exclusive);
    let allowance_keys_to_remove: Vec<_> = ALLOWANCES
        .range(deps.storage, start.clone(), None, Order::Ascending)
        .take(limit)
        .filter_map(|result| {
            let (key, allowance) = result.ok()?;
            if allowance.amount.is_zero() {
                Some(key)
            } else {
                None
            }
        })
        .collect();

    for key in allowance_keys_to_remove {
        ALLOWANCES.remove(deps.storage, key);
    }

    // Remove Balances with a value of zero
    let balances_keys_to_remove: Vec<_> = BALANCES
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .filter_map(|result| {
            let (key, balance) = result.ok()?;
            if balance.is_zero() {
                Some(key)
            } else {
                None
            }
        })
        .collect();

    for key in balances_keys_to_remove {
        BALANCES.remove(deps.storage, key);
    }

    Ok(Response::new().add_attribute("action", "execute_remove_zero_state_values"))
}

pub fn execute_normalize_balance_keys(
    deps: DepsMut,
    info: MessageInfo,
    start_after: Option<SerializedBalanceKey>,
    limit: Option<u32>,
) -> Result<Response, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        admin.general_admin == info.sender,
        ContractError::Unauthorized {}
    );

    let limit = limit.unwrap_or(100) as usize;
    let start = start_after.map(Bound::exclusive);
    let mut normalized_count: u32 = 0;

    // Normalize BALANCES
    let balance_entries: Vec<(SerializedBalanceKey, Uint128)> = BALANCES
        .range(deps.storage, start.clone(), None, Order::Ascending)
        .take(limit)
        .filter_map(|result| result.ok())
        .collect();

    for (key, balance) in balance_entries {
        let (chain_uid, address, token_id) = key.clone();
        let lowercase_address = address.to_lowercase();
        if lowercase_address != address {
            BALANCES.remove(deps.storage, key);
            let normalized_key: SerializedBalanceKey =
                (chain_uid, lowercase_address, token_id);
            let existing = BALANCES
                .may_load(deps.storage, normalized_key.clone())?
                .unwrap_or(Uint128::zero());
            let combined = existing.checked_add(balance)?;
            if !combined.is_zero() {
                BALANCES.save(deps.storage, normalized_key, &combined)?;
            }
            normalized_count += 1;
        }
    }

    // Normalize ALLOWANCES
    let allowance_entries: Vec<(SerializedBalanceKey, _)> = ALLOWANCES
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .filter_map(|result| result.ok())
        .collect();

    for (key, allowance) in allowance_entries {
        let (chain_uid, address, token_id) = key.clone();
        let lowercase_address = address.to_lowercase();
        if lowercase_address != address {
            ALLOWANCES.remove(deps.storage, key);
            let normalized_key: SerializedBalanceKey =
                (chain_uid, lowercase_address, token_id);
            if !ALLOWANCES.has(deps.storage, normalized_key.clone()) {
                ALLOWANCES.save(deps.storage, normalized_key, &allowance)?;
            }
            normalized_count += 1;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "execute_normalize_balance_keys")
        .add_attribute("normalized_count", normalized_count.to_string()))
}
