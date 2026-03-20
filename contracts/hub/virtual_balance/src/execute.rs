use cosmwasm_std::{
    ensure, Addr, Attribute, DepsMut, Env, MessageInfo, Order, Response, Uint256, WasmMsg,
};
use cw_storage_plus::Bound;
use euclid::{
    admin::{self, AdminType},
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{
        escrow_balance_change_event, token_metadata_update_event, virtual_balance_change_event,
    },
    msgs::{
        hook::VoucherReceive,
        virtual_balance::msg::{ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteTransfer},
    },
    normalize::{normalize_token_to_voucher, normalize_voucher_to_token},
    token::{TokenMetadata, TokenType},
    voucher::{BalanceKey, SerializedBalanceKey},
};

use crate::state::{
    get_escrow_balance_key, get_token_metadata_key, VoucherAllowance, ADMIN, ESCROW_BALANCES,
    STATE, VOUCHER_ALLOWANCES, VOUCHER_BALANCES, VOUCHER_DECIMAL,
};

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

    let metadata_key = get_token_metadata_key(
        msg.balance_key.token_id.clone(),
        msg.token_source_chain_uid.clone(),
        msg.token_type.clone(),
    );
    let metadata = metadata_key.load(deps.storage)?;
    let normalized_voucher_amount =
        normalize_token_to_voucher(msg.amount, metadata.token_type.get_decimals()?)?;

    let key = msg.balance_key.clone().to_serialized_balance_key();

    let old_balance = VOUCHER_BALANCES
        .may_load(deps.storage, key.clone())?
        .unwrap_or_default();

    let new_balance = old_balance.checked_add(normalized_voucher_amount)?;

    VOUCHER_BALANCES.save(deps.storage, key, &new_balance)?;

    // Increment escrow balance (stores raw token amounts, not normalized)
    let escrow_key = get_escrow_balance_key(
        msg.balance_key.token_id.clone(),
        msg.token_source_chain_uid,
        msg.token_type,
    );
    let old_escrow = escrow_key.may_load(deps.storage)?.unwrap_or_default();
    let new_escrow = old_escrow.checked_add(msg.amount)?;
    escrow_key.save(deps.storage, &new_escrow)?;

    let response = Response::new()
        .add_attribute("action", "execute_mint")
        .add_attribute("mint_amount", msg.amount)
        .add_attribute("normalized_amount", normalized_voucher_amount)
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
    ensure!(
        !msg.voucher_amount.is_zero(),
        ContractError::ZeroAssetAmount {}
    );

    let key = BalanceKey {
        token_id: msg.token_id.clone(),
        cross_chain_user: msg.from_user.clone(),
    }
    .to_serialized_balance_key();

    let old_balance = VOUCHER_BALANCES
        .may_load(deps.storage, key.clone())?
        .ok_or(ContractError::BalanceNotFound {
            key: format!("{:?}", key),
        })?;

    let new_balance = old_balance.checked_sub(msg.voucher_amount)?;

    if new_balance.is_zero() {
        VOUCHER_BALANCES.remove(deps.storage, key);
    } else {
        VOUCHER_BALANCES.save(deps.storage, key, &new_balance)?;
    }

    // Decrement escrow balance: denormalize voucher amount to raw token amount
    let metadata_key = get_token_metadata_key(
        msg.token_id.clone(),
        msg.release_chain_uid.clone(),
        msg.release_denom.clone(),
    );
    let metadata = metadata_key.load(deps.storage)?;
    let raw_token_amount =
        normalize_voucher_to_token(msg.voucher_amount, metadata.token_type.get_decimals()?)?;

    let escrow_key = get_escrow_balance_key(
        msg.token_id.clone(),
        msg.release_chain_uid.clone(),
        msg.release_denom.clone(),
    );
    let old_escrow_balance = escrow_key.load(deps.storage)?;
    let new_escrow_balance = old_escrow_balance.checked_sub(raw_token_amount)?;

    escrow_key.save(deps.storage, &new_escrow_balance)?;

    Ok(Response::new()
        .add_event(virtual_balance_change_event(
            "voucher_burn",
            &msg.voucher_amount,
            &msg.from_user,
            &msg.token_id,
        ))
        .add_event(escrow_balance_change_event(
            "voucher_burn",
            &new_escrow_balance,
            &msg.token_id,
            &msg.release_chain_uid,
            &msg.release_denom,
        ))
        .add_attribute("action", "execute_burn")
        .add_attribute("burn_amount", msg.voucher_amount)
        .add_attribute("raw_token_amount", raw_token_amount)
        .add_attribute("burn_address", msg.from_user.to_sender_string())
        .add_attribute("burn_address_chain", msg.from_user.chain_uid.to_string())
        .add_attribute("burn_token_id", msg.token_id)
        .add_attribute("new_balance", new_balance))
}

pub fn execute_transfer(
    deps: &mut DepsMut,
    env: Env,
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

    let mut response = if let Some(from) = transfer_msg.from {
        let attributes = _deduct_allowance(
            deps,
            &env,
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
        // Send message to receiver. Caution should be taken to not trigger a message with wrong chain uid as chain uid is not verified here.
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
    amount: Uint256,
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
    let sender_old_balance = VOUCHER_BALANCES
        .may_load(deps.storage, sender_key.clone())?
        .unwrap_or(Uint256::zero());

    ensure!(
        sender_old_balance.ge(&amount),
        ContractError::new(&format!(
            "Not Enough Funds: {} < {}",
            sender_old_balance, amount
        ))
    );

    let sender_new_balance = sender_old_balance.checked_sub(amount)?;
    if sender_new_balance.is_zero() {
        VOUCHER_BALANCES.remove(deps.storage, sender_key);
    } else {
        VOUCHER_BALANCES.save(deps.storage, sender_key, &sender_new_balance)?;
    }

    let receiver_balance_key = BalanceKey {
        token_id: token_id.clone(),
        cross_chain_user: to.clone(),
    };
    let receiver_key = receiver_balance_key.clone().to_serialized_balance_key();

    // Increase receiver balance
    let receiver_old_balance = VOUCHER_BALANCES
        .may_load(deps.storage, receiver_key.clone())?
        .unwrap_or(Uint256::zero());
    let receiver_new_balance = receiver_old_balance.checked_add(amount)?;
    VOUCHER_BALANCES.save(deps.storage, receiver_key, &receiver_new_balance)?;

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
    env: &Env,
    sender: &CrossChainUser,
    from: &CrossChainUser,
    amount: Uint256,
    token_id: &str,
) -> Result<Vec<Attribute>, ContractError> {
    let sender_balance_key = BalanceKey {
        token_id: token_id.to_string(),
        cross_chain_user: from.clone(),
    };
    let serialized_balance_key = sender_balance_key.clone().to_serialized_balance_key();
    let mut allowance = VOUCHER_ALLOWANCES
        .load(deps.storage, serialized_balance_key.clone())
        .unwrap_or(VoucherAllowance {
            amount: Uint256::zero(),
            spender: from.clone(),
            expires_at: None,
        });

    ensure!(
        allowance.spender == sender.clone(),
        ContractError::Unauthorized {}
    );

    // Check expiry
    if let Some(expires_at) = allowance.expires_at {
        ensure!(
            env.block.time < expires_at,
            ContractError::new("Allowance has expired")
        );
    }

    ensure!(
        allowance.amount.ge(&amount),
        ContractError::new(&format!(
            "Not Enough Allowance: {} < {}",
            allowance.amount, amount
        ))
    );

    allowance.amount = allowance.amount.checked_sub(amount)?;
    if allowance.amount.is_zero() {
        VOUCHER_ALLOWANCES.remove(deps.storage, serialized_balance_key);
    } else {
        VOUCHER_ALLOWANCES.save(deps.storage, serialized_balance_key, &allowance)?;
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
    VOUCHER_ALLOWANCES.save(
        deps.storage,
        key.to_serialized_balance_key(),
        &VoucherAllowance {
            amount: msg.amount,
            spender: spender.clone(),
            expires_at: None,
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
    let allowance_keys_to_remove: Vec<_> = VOUCHER_ALLOWANCES
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
        VOUCHER_ALLOWANCES.remove(deps.storage, key);
    }

    // Remove Balances with a value of zero
    let balances_keys_to_remove: Vec<_> = VOUCHER_BALANCES
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
        VOUCHER_BALANCES.remove(deps.storage, key);
    }

    Ok(Response::new().add_attribute("action", "execute_remove_zero_state_values"))
}

pub fn execute_register_token_metadata(
    deps: DepsMut,
    info: MessageInfo,
    token_metadata: TokenMetadata,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    let token = token_metadata.token.validate()?;
    let chain_uid = token_metadata.chain_uid.validate()?;
    let decimals = token_metadata.token_type.get_decimals()?;
    ensure!(
        decimals.le(&VOUCHER_DECIMAL),
        ContractError::InvalidDecimals { decimals }
    );

    let token_metadata_storage = get_token_metadata_key(
        token.to_string(),
        chain_uid.clone(),
        token_metadata.token_type.clone(),
    );

    // If token is already registered, check if decimals match
    // If not, return an error
    // If yes, update the token metadata
    if token_metadata_storage.has(deps.storage) {
        let existing = token_metadata_storage.load(deps.storage)?;
        let existing_decimals = existing.token_type.get_decimals()?;
        ensure!(
            existing_decimals == decimals,
            ContractError::DecimalsMismatch {
                expected: existing_decimals,
                received: decimals,
            }
        );
    }
    token_metadata_storage.save(deps.storage, &token_metadata)?;

    // Add escrow balance for the token if not already present
    ESCROW_BALANCES.update(
        deps.storage,
        (
            token.to_string(),
            chain_uid.clone(),
            token_metadata.token_type.get_key(),
        ),
        |maybe_escrow| {
            if let Some(exiting_balance) = maybe_escrow {
                Ok::<Uint256, ContractError>(exiting_balance)
            } else {
                Ok(Uint256::zero())
            }
        },
    )?;

    Ok(Response::new().add_event(token_metadata_update_event(
        &token_metadata,
        "register_denom",
    )))
}

pub fn execute_deregister_token_metadata(
    deps: DepsMut,
    info: MessageInfo,
    token_id: String,
    chain_uid: ChainUid,
    token_type: TokenType,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    ensure!(info.sender == state.router, ContractError::Unauthorized {});

    let token_metadata_key =
        get_token_metadata_key(token_id.clone(), chain_uid.clone(), token_type.clone());

    let mut token_metadata = token_metadata_key
        .load(deps.storage)
        .map_err(|e| ContractError::new(&format!("Failed to load token metadata: {}", e)))?;
    ensure!(
        token_metadata.allowed,
        ContractError::new("Token already deregistered")
    );
    token_metadata.allowed = false;
    token_metadata_key.save(deps.storage, &token_metadata)?;

    Ok(Response::new().add_event(token_metadata_update_event(
        &token_metadata,
        "deregister_denom",
    )))
}
