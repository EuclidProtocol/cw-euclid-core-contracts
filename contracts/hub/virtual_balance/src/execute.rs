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
    // Reject mixed-case or empty addresses before mutating state
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
    // Reject mixed-case or empty addresses before mutating state
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
    // Validate all CrossChainUser fields to reject mixed-case or empty addresses
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
        .add_attribute("transfer_from_address", from.to_sender_string())
        .add_attribute("transfer_from_chain", from.chain_uid.to_string())
        .add_attribute("transfer_to_address", to.to_sender_string())
        .add_attribute("transfer_to_chain", to.chain_uid.to_string())
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
    let admin_type_str = format!("{admin_type:?}");
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
        .add_attribute("new_admin", new_admin.to_string())
        .add_attribute("admin_type", admin_type_str))
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
    let old_router = state.router.clone();
    state.router = verified_router.clone();
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("action", "execute_update_router")
        .add_attribute("old_router", old_router)
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
    // Reject mixed-case or empty addresses before mutating state
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

    let removed_allowances = allowance_keys_to_remove.len();
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

    let removed_balances = balances_keys_to_remove.len();
    for key in balances_keys_to_remove {
        BALANCES.remove(deps.storage, key);
    }

    Ok(Response::new()
        .add_attribute("action", "execute_remove_zero_state_values")
        .add_attribute("removed_balances", removed_balances.to_string())
        .add_attribute("removed_allowances", removed_allowances.to_string()))
}

/// Migrates mixed-case balance/allowance keys to lowercase.
/// Call repeatedly with `start_after: None` until both
/// `normalized_balances` and `normalized_allowances` return "0".
/// Note: the `start_after` cursor applies independently to both BALANCES
/// and ALLOWANCES ranges, so for simplest usage pass `None` each call.
pub fn execute_normalize_balance_keys(
    deps: DepsMut,
    info: MessageInfo,
    skip: Option<u32>,
    limit: Option<u32>,
) -> Result<Response, ContractError> {
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        admin.general_admin == info.sender,
        ContractError::Unauthorized {}
    );

    // Default to 100 to stay within gas limits on production chains
    let limit = limit.unwrap_or(100) as usize;
    let skip = skip.unwrap_or(0) as usize;
    let mut normalized_balances: u32 = 0;
    let mut normalized_allowances: u32 = 0;
    let mut skipped_errors: u32 = 0;

    // Collect first to avoid mutating storage while iterating.
    // Deserialization errors are counted (skipped_errors) rather than silently dropped.
    let mut balance_entries: Vec<(SerializedBalanceKey, Uint128)> = Vec::new();
    for result in BALANCES
        .range(deps.storage, None, None, Order::Ascending)
        .skip(skip)
        .take(limit)
    {
        match result {
            Ok(entry) => balance_entries.push(entry),
            Err(_) => skipped_errors += 1,
        }
    }

    for (key, balance) in balance_entries {
        let (chain_uid, address, token_id) = key.clone();
        let lowercase_address = address.to_lowercase();
        if lowercase_address != address {
            // Remove the mixed-case entry
            BALANCES.remove(deps.storage, key);
            // Add balance to the lowercase key, combining with any existing balance
            let normalized_key: SerializedBalanceKey = (chain_uid, lowercase_address, token_id);
            BALANCES.update(deps.storage, normalized_key, |existing| {
                let combined = existing.unwrap_or(Uint128::zero()).checked_add(balance)?;
                Ok::<_, ContractError>(combined)
            })?;
            normalized_balances += 1;
        }
    }

    // Collect first to avoid mutating storage while iterating
    let mut allowance_entries: Vec<(SerializedBalanceKey, _)> = Vec::new();
    for result in ALLOWANCES
        .range(deps.storage, None, None, Order::Ascending)
        .skip(skip)
        .take(limit)
    {
        match result {
            Ok(entry) => allowance_entries.push(entry),
            Err(_) => skipped_errors += 1,
        }
    }

    for (key, allowance) in allowance_entries {
        let (chain_uid, address, token_id) = key.clone();
        let lowercase_address = address.to_lowercase();
        if lowercase_address != address {
            ALLOWANCES.remove(deps.storage, key);
            let normalized_key: SerializedBalanceKey = (chain_uid, lowercase_address, token_id);
            // On collision, keep the higher allowance to avoid silently dropping approved value
            if let Some(existing) = ALLOWANCES.may_load(deps.storage, normalized_key.clone())? {
                let merged = Allowance {
                    amount: existing.amount.max(allowance.amount),
                    spender: existing.spender,
                };
                ALLOWANCES.save(deps.storage, normalized_key, &merged)?;
            } else {
                ALLOWANCES.save(deps.storage, normalized_key, &allowance)?;
            }
            normalized_allowances += 1;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "execute_normalize_balance_keys")
        .add_attribute("normalized_balances", normalized_balances.to_string())
        .add_attribute("normalized_allowances", normalized_allowances.to_string())
        .add_attribute("skipped_errors", skipped_errors.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::helpers::{
        init, remote_user, seed_allowance, seed_balance, vsl_user, TEST_ROUTER,
    };
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env};
    use cosmwasm_std::{attr, Addr};
    use euclid::msgs::virtual_balance::msg::{
        ExecuteApprove, ExecuteBurn, ExecuteMint, ExecuteTransfer,
    };

    // -----------------------------------------------------------------------
    // Helpers — build MessageInfo without holding a borrow of `deps`.
    // We use addr_make on a temporary api reference and immediately convert
    // to an owned Addr, so the borrow is released before as_mut() is called.
    // -----------------------------------------------------------------------

    /// Return the router address as an owned `Addr`.
    fn router_addr_for(
        deps: &cosmwasm_std::OwnedDeps<
            cosmwasm_std::testing::MockStorage,
            cosmwasm_std::testing::MockApi,
            cosmwasm_std::testing::MockQuerier,
        >,
    ) -> Addr {
        deps.api.addr_make(TEST_ROUTER)
    }

    // -----------------------------------------------------------------------
    // execute_mint – happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_mint_increases_balance() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = remote_user("1", "cosmos1alice");
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let res = execute_mint(
            deps.as_mut(),
            info,
            ExecuteMint {
                amount: Uint128::new(500),
                balance_key: BalanceKey {
                    cross_chain_user: user.clone(),
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap();

        assert!(res
            .attributes
            .iter()
            .any(|a| a == &attr("action", "execute_mint")));
        assert!(res
            .attributes
            .iter()
            .any(|a| a == &attr("mint_amount", "500")));

        let key = BalanceKey {
            cross_chain_user: user,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        assert_eq!(
            BALANCES.load(&deps.storage, key).unwrap(),
            Uint128::new(500)
        );
    }

    #[test]
    fn test_mint_accumulates_on_second_call() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = remote_user("1", "cosmos1alice");
        let bk = BalanceKey {
            cross_chain_user: user.clone(),
            token_id: "eucl".to_string(),
        };

        {
            let router = router_addr_for(&deps);
            let info = message_info(&router, &[]);
            execute_mint(
                deps.as_mut(),
                info,
                ExecuteMint {
                    amount: Uint128::new(300),
                    balance_key: bk.clone(),
                },
            )
            .unwrap();
        }
        {
            let router = router_addr_for(&deps);
            let info = message_info(&router, &[]);
            execute_mint(
                deps.as_mut(),
                info,
                ExecuteMint {
                    amount: Uint128::new(200),
                    balance_key: bk.clone(),
                },
            )
            .unwrap();
        }

        assert_eq!(
            BALANCES
                .load(&deps.storage, bk.to_serialized_balance_key())
                .unwrap(),
            Uint128::new(500)
        );
    }

    // -----------------------------------------------------------------------
    // execute_mint – error paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_mint_unauthorized_non_router() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let non_router = deps.api.addr_make("non_router");
        let info = message_info(&non_router, &[]);
        let err = execute_mint(
            deps.as_mut(),
            info,
            ExecuteMint {
                amount: Uint128::new(100),
                balance_key: BalanceKey {
                    cross_chain_user: remote_user("1", "cosmos1alice"),
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_mint_zero_amount_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_mint(
            deps.as_mut(),
            info,
            ExecuteMint {
                amount: Uint128::zero(),
                balance_key: BalanceKey {
                    cross_chain_user: remote_user("1", "cosmos1alice"),
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_mint_rejects_mixed_case_address() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let mixed = CrossChainUser::new(
            ChainUid::create("cosmos".to_string()).unwrap(),
            "Cosmos1AbC".to_string(),
        );
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_mint(
            deps.as_mut(),
            info,
            ExecuteMint {
                amount: Uint128::new(100),
                balance_key: BalanceKey {
                    cross_chain_user: mixed,
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Address must be lowercase"));
    }

    // -----------------------------------------------------------------------
    // execute_burn – happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_burn_decreases_balance() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = remote_user("1", "cosmos1alice");
        seed_balance(&mut deps, user.clone(), "eucl", 1000);

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        execute_burn(
            deps.as_mut(),
            info,
            ExecuteBurn {
                amount: Uint128::new(400),
                balance_key: BalanceKey {
                    cross_chain_user: user.clone(),
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap();

        let key = BalanceKey {
            cross_chain_user: user,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        assert_eq!(
            BALANCES.load(&deps.storage, key).unwrap(),
            Uint128::new(600)
        );
    }

    #[test]
    fn test_burn_to_zero_removes_entry() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = remote_user("1", "cosmos1alice");
        seed_balance(&mut deps, user.clone(), "eucl", 500);

        let bk = BalanceKey {
            cross_chain_user: user,
            token_id: "eucl".to_string(),
        };
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        execute_burn(
            deps.as_mut(),
            info,
            ExecuteBurn {
                amount: Uint128::new(500),
                balance_key: bk.clone(),
            },
        )
        .unwrap();

        assert!(BALANCES
            .load(&deps.storage, bk.to_serialized_balance_key())
            .is_err());
    }

    // -----------------------------------------------------------------------
    // execute_burn – error paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_burn_unauthorized_non_router() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let non_router = deps.api.addr_make("non_router");
        let info = message_info(&non_router, &[]);
        let err = execute_burn(
            deps.as_mut(),
            info,
            ExecuteBurn {
                amount: Uint128::new(100),
                balance_key: BalanceKey {
                    cross_chain_user: remote_user("1", "cosmos1alice"),
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_burn_zero_amount_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_burn(
            deps.as_mut(),
            info,
            ExecuteBurn {
                amount: Uint128::zero(),
                balance_key: BalanceKey {
                    cross_chain_user: remote_user("1", "cosmos1alice"),
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_burn_nonexistent_balance_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_burn(
            deps.as_mut(),
            info,
            ExecuteBurn {
                amount: Uint128::new(1),
                balance_key: BalanceKey {
                    cross_chain_user: remote_user("1", "cosmos1alice"),
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::BalanceNotFound { .. }));
    }

    #[test]
    fn test_burn_overdraft_returns_error() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = remote_user("1", "cosmos1alice");
        seed_balance(&mut deps, user.clone(), "eucl", 100);

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_burn(
            deps.as_mut(),
            info,
            ExecuteBurn {
                amount: Uint128::new(200),
                balance_key: BalanceKey {
                    cross_chain_user: user,
                    token_id: "eucl".to_string(),
                },
            },
        )
        .unwrap_err();
        // checked_sub produces an OverflowError ("Cannot Sub with X and Y")
        assert!(matches!(err, ContractError::Overflow(_)));
    }

    // -----------------------------------------------------------------------
    // execute_transfer – happy path (router-set sender, no from/allowance)
    // -----------------------------------------------------------------------

    #[test]
    fn test_transfer_direct_decrements_sender_increments_receiver() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let from = remote_user("1", "cosmos1sender");
        let to = remote_user("1", "cosmos1receiver");
        seed_balance(&mut deps, from.clone(), "eucl", 800);

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let mut deps_mut = deps.as_mut();
        execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(300),
                token_id: "eucl".to_string(),
                sender: Some(from.clone()),
                to: to.clone(),
                from: None,
                msg: None,
            },
        )
        .unwrap();

        let from_key = BalanceKey {
            cross_chain_user: from,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        let to_key = BalanceKey {
            cross_chain_user: to,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();

        assert_eq!(
            BALANCES.load(deps_mut.storage, from_key).unwrap(),
            Uint128::new(500)
        );
        assert_eq!(
            BALANCES.load(deps_mut.storage, to_key).unwrap(),
            Uint128::new(300)
        );
    }

    #[test]
    fn test_transfer_to_zero_removes_sender_entry() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let from = remote_user("1", "cosmos1sender");
        let to = remote_user("1", "cosmos1receiver");
        seed_balance(&mut deps, from.clone(), "eucl", 100);

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let mut deps_mut = deps.as_mut();
        execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(100),
                token_id: "eucl".to_string(),
                sender: Some(from.clone()),
                to: to.clone(),
                from: None,
                msg: None,
            },
        )
        .unwrap();

        let from_key = BalanceKey {
            cross_chain_user: from,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        assert!(BALANCES.load(deps_mut.storage, from_key).is_err());
    }

    // -----------------------------------------------------------------------
    // execute_transfer – error paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_transfer_zero_amount_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let from = remote_user("1", "cosmos1sender");
        let to = remote_user("1", "cosmos1receiver");
        seed_balance(&mut deps, from.clone(), "eucl", 500);

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let mut deps_mut = deps.as_mut();
        let err = execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::zero(),
                token_id: "eucl".to_string(),
                sender: Some(from),
                to,
                from: None,
                msg: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_transfer_same_sender_and_receiver_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let user = remote_user("1", "cosmos1user");
        seed_balance(&mut deps, user.clone(), "eucl", 500);

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let mut deps_mut = deps.as_mut();
        let err = execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(100),
                token_id: "eucl".to_string(),
                sender: Some(user.clone()),
                to: user,
                from: None,
                msg: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::SameAddress {});
    }

    #[test]
    fn test_transfer_pseudo_sender_only_allowed_by_router() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let from = remote_user("1", "cosmos1sender");
        let to = remote_user("1", "cosmos1receiver");

        let not_router = deps.api.addr_make("not_router");
        let info = message_info(&not_router, &[]);
        let mut deps_mut = deps.as_mut();
        let err = execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(100),
                token_id: "eucl".to_string(),
                sender: Some(from),
                to,
                from: None,
                msg: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UnauthorizedWithMsg { .. }));
    }

    // -----------------------------------------------------------------------
    // execute_transfer with allowance (from field set)
    // -----------------------------------------------------------------------

    #[test]
    fn test_transfer_via_allowance_deducts_allowance() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let owner = vsl_user("alice");
        let spender = vsl_user("bob");
        let recipient = remote_user("1", "cosmos1eve");
        seed_balance(&mut deps, owner.clone(), "eucl", 1000);
        seed_allowance(&mut deps, owner.clone(), "eucl", spender.clone(), 400);

        let spender_addr = Addr::unchecked(spender.address.clone());
        let info = message_info(&spender_addr, &[]);
        let mut deps_mut = deps.as_mut();
        execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(150),
                token_id: "eucl".to_string(),
                sender: None,
                to: recipient.clone(),
                from: Some(owner.clone()),
                msg: None,
            },
        )
        .unwrap();

        let allowance_key = BalanceKey {
            cross_chain_user: owner.clone(),
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        let remaining = ALLOWANCES.load(deps_mut.storage, allowance_key).unwrap();
        assert_eq!(remaining.amount, Uint128::new(250));

        let owner_key = BalanceKey {
            cross_chain_user: owner,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        assert_eq!(
            BALANCES.load(deps_mut.storage, owner_key).unwrap(),
            Uint128::new(850)
        );
    }

    #[test]
    fn test_transfer_via_allowance_removes_allowance_when_exhausted() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let owner = vsl_user("alice");
        let spender = vsl_user("bob");
        let recipient = remote_user("1", "cosmos1eve");
        seed_balance(&mut deps, owner.clone(), "eucl", 500);
        seed_allowance(&mut deps, owner.clone(), "eucl", spender.clone(), 200);

        let spender_addr = Addr::unchecked(spender.address.clone());
        let info = message_info(&spender_addr, &[]);
        let mut deps_mut = deps.as_mut();
        execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(200),
                token_id: "eucl".to_string(),
                sender: None,
                to: recipient,
                from: Some(owner.clone()),
                msg: None,
            },
        )
        .unwrap();

        let allowance_key = BalanceKey {
            cross_chain_user: owner,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        assert!(ALLOWANCES.load(deps_mut.storage, allowance_key).is_err());
    }

    #[test]
    fn test_transfer_via_allowance_insufficient_allowance_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let owner = vsl_user("alice");
        let spender = vsl_user("bob");
        let recipient = remote_user("1", "cosmos1eve");
        seed_balance(&mut deps, owner.clone(), "eucl", 1000);
        seed_allowance(&mut deps, owner.clone(), "eucl", spender.clone(), 50);

        let spender_addr = Addr::unchecked(spender.address.clone());
        let info = message_info(&spender_addr, &[]);
        let mut deps_mut = deps.as_mut();
        let err = execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(100),
                token_id: "eucl".to_string(),
                sender: None,
                to: recipient,
                from: Some(owner),
                msg: None,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Not Enough Allowance"));
    }

    // -----------------------------------------------------------------------
    // execute_approve – happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_approve_by_router_sets_allowance() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let owner = vsl_user("alice");
        let spender = vsl_user("bob");

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        execute_approve(
            deps.as_mut(),
            info,
            ExecuteApprove {
                amount: Uint128::new(500),
                token_id: "eucl".to_string(),
                spender: spender.clone(),
                owner: owner.clone(),
            },
        )
        .unwrap();

        let key = BalanceKey {
            cross_chain_user: owner,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        let allowance = ALLOWANCES.load(&deps.storage, key).unwrap();
        assert_eq!(allowance.amount, Uint128::new(500));
        assert_eq!(allowance.spender, spender);
    }

    #[test]
    fn test_approve_by_vsl_user_themselves() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let alice_addr = deps.api.addr_make("alice");
        let owner = vsl_user(&alice_addr.to_string());
        let spender = vsl_user("bob");

        let info = message_info(&alice_addr, &[]);
        execute_approve(
            deps.as_mut(),
            info,
            ExecuteApprove {
                amount: Uint128::new(100),
                token_id: "eucl".to_string(),
                spender: spender.clone(),
                owner: owner.clone(),
            },
        )
        .unwrap();

        let key = BalanceKey {
            cross_chain_user: owner,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        let allowance = ALLOWANCES.load(&deps.storage, key).unwrap();
        assert_eq!(allowance.amount, Uint128::new(100));
    }

    // -----------------------------------------------------------------------
    // execute_approve – error paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_approve_zero_amount_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_approve(
            deps.as_mut(),
            info,
            ExecuteApprove {
                amount: Uint128::zero(),
                token_id: "eucl".to_string(),
                spender: vsl_user("bob"),
                owner: vsl_user("alice"),
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_approve_same_owner_and_spender_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let alice = vsl_user("alice");
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_approve(
            deps.as_mut(),
            info,
            ExecuteApprove {
                amount: Uint128::new(100),
                token_id: "eucl".to_string(),
                spender: alice.clone(),
                owner: alice,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::SameAddress {});
    }

    #[test]
    fn test_approve_unauthorized_non_router_non_owner() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let third_party = deps.api.addr_make("third_party");
        let info = message_info(&third_party, &[]);
        let err = execute_approve(
            deps.as_mut(),
            info,
            ExecuteApprove {
                amount: Uint128::new(100),
                token_id: "eucl".to_string(),
                spender: vsl_user("bob"),
                owner: vsl_user("alice"),
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // execute_update_router
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_router_by_admin_succeeds() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let new_router = deps.api.addr_make("new_router");
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        execute_update_router(deps.as_mut(), info, new_router.clone()).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.router, new_router);
    }

    #[test]
    fn test_update_router_by_non_admin_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let new_router = deps.api.addr_make("new_router");
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);

        let err = execute_update_router(deps.as_mut(), info, new_router).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // execute_update_admin
    // -----------------------------------------------------------------------

    #[test]
    fn test_update_general_admin_by_current_general_admin() {
        use euclid::admin::AdminType;
        let mut deps = mock_dependencies();
        init(&mut deps);
        let new_admin_addr = deps.api.addr_make("new_general_admin");
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);

        execute_update_admin(
            deps.as_mut(),
            mock_env(),
            info,
            new_admin_addr.to_string(),
            AdminType::GeneralAdmin,
        )
        .unwrap();

        let admin = ADMIN.load(&deps.storage).unwrap();
        assert_eq!(admin.general_admin, new_admin_addr);
    }

    #[test]
    fn test_update_general_admin_by_wrong_role_rejected() {
        use euclid::admin::AdminType;
        let mut deps = mock_dependencies();
        init(&mut deps);
        let new_admin_addr = deps.api.addr_make("new_admin");
        let other = deps.api.addr_make("other");
        let info = message_info(&other, &[]);
        let err = execute_update_admin(
            deps.as_mut(),
            mock_env(),
            info,
            new_admin_addr.to_string(),
            AdminType::GeneralAdmin,
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::UnauthorizedWithMsg { .. }));
    }

    // -----------------------------------------------------------------------
    // execute_remove_zero_state_values
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_zero_state_values_prunes_zero_balances_and_allowances() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let user_zero = vsl_user("zerouser");
        let user_nonzero = vsl_user("nonzerouser");
        let zero_key = BalanceKey {
            cross_chain_user: user_zero,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        let nonzero_key = BalanceKey {
            cross_chain_user: user_nonzero,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        BALANCES
            .save(&mut deps.storage, zero_key.clone(), &Uint128::zero())
            .unwrap();
        BALANCES
            .save(&mut deps.storage, nonzero_key.clone(), &Uint128::new(100))
            .unwrap();

        let owner_zero = vsl_user("zeroowner");
        let owner_nonzero = vsl_user("nonzeroowner");
        let spender = vsl_user("spender");
        let allowance_zero_key = BalanceKey {
            cross_chain_user: owner_zero,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        let allowance_nonzero_key = BalanceKey {
            cross_chain_user: owner_nonzero,
            token_id: "eucl".to_string(),
        }
        .to_serialized_balance_key();
        ALLOWANCES
            .save(
                &mut deps.storage,
                allowance_zero_key.clone(),
                &Allowance {
                    amount: Uint128::zero(),
                    spender: spender.clone(),
                },
            )
            .unwrap();
        ALLOWANCES
            .save(
                &mut deps.storage,
                allowance_nonzero_key.clone(),
                &Allowance {
                    amount: Uint128::new(50),
                    spender,
                },
            )
            .unwrap();

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        execute_remove_zero_state_values(deps.as_mut(), info, None, None).unwrap();

        assert!(BALANCES.load(&deps.storage, zero_key).is_err());
        assert!(BALANCES.load(&deps.storage, nonzero_key).is_ok());
        assert!(ALLOWANCES.load(&deps.storage, allowance_zero_key).is_err());
        assert!(ALLOWANCES
            .load(&deps.storage, allowance_nonzero_key)
            .is_ok());
    }

    #[test]
    fn test_remove_zero_state_values_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);
        let err = execute_remove_zero_state_values(deps.as_mut(), info, None, None).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    // -----------------------------------------------------------------------
    // execute_normalize_balance_keys
    // -----------------------------------------------------------------------

    #[test]
    fn test_normalize_balance_keys_lowercases_mixed_case_entry() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let mixed_key: (ChainUid, String, String) = (
            ChainUid::create("cosmos".to_string()).unwrap(),
            "Cosmos1ABC".to_string(),
            "eucl".to_string(),
        );
        BALANCES
            .save(&mut deps.storage, mixed_key.clone(), &Uint128::new(250))
            .unwrap();

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let res = execute_normalize_balance_keys(deps.as_mut(), info, None, None).unwrap();

        let norm_count: u32 = res
            .attributes
            .iter()
            .find(|a| a.key == "normalized_balances")
            .unwrap()
            .value
            .parse()
            .unwrap();
        assert_eq!(norm_count, 1);

        assert!(BALANCES.load(&deps.storage, mixed_key).is_err());
        let lower_key: (ChainUid, String, String) = (
            ChainUid::create("cosmos".to_string()).unwrap(),
            "cosmos1abc".to_string(),
            "eucl".to_string(),
        );
        assert_eq!(
            BALANCES.load(&deps.storage, lower_key).unwrap(),
            Uint128::new(250)
        );
    }

    #[test]
    fn test_normalize_balance_keys_combines_with_existing_lowercase() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let chain = ChainUid::create("cosmos".to_string()).unwrap();
        let mixed_key: (ChainUid, String, String) =
            (chain.clone(), "Owner".to_string(), "eucl".to_string());
        let lower_key: (ChainUid, String, String) =
            (chain, "owner".to_string(), "eucl".to_string());

        BALANCES
            .save(&mut deps.storage, mixed_key, &Uint128::new(100))
            .unwrap();
        BALANCES
            .save(&mut deps.storage, lower_key.clone(), &Uint128::new(50))
            .unwrap();

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        execute_normalize_balance_keys(deps.as_mut(), info, None, None).unwrap();

        assert_eq!(
            BALANCES.load(&deps.storage, lower_key).unwrap(),
            Uint128::new(150)
        );
    }

    #[test]
    fn test_normalize_balance_keys_noop_for_already_lowercase() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let key: (ChainUid, String, String) = (
            ChainUid::create("cosmos".to_string()).unwrap(),
            "alreadylower".to_string(),
            "eucl".to_string(),
        );
        BALANCES
            .save(&mut deps.storage, key, &Uint128::new(100))
            .unwrap();

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let res = execute_normalize_balance_keys(deps.as_mut(), info, None, None).unwrap();
        let norm: u32 = res
            .attributes
            .iter()
            .find(|a| a.key == "normalized_balances")
            .unwrap()
            .value
            .parse()
            .unwrap();
        assert_eq!(norm, 0);
    }

    #[test]
    fn test_normalize_balance_keys_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let not_admin = deps.api.addr_make("not_admin");
        let info = message_info(&not_admin, &[]);
        let err = execute_normalize_balance_keys(deps.as_mut(), info, None, None).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn test_normalize_allowance_keys_keeps_max_on_collision() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        let chain = ChainUid::create("cosmos".to_string()).unwrap();
        let spender = vsl_user("spender");
        let mixed_key: (ChainUid, String, String) =
            (chain.clone(), "Owner".to_string(), "eucl".to_string());
        let lower_key: (ChainUid, String, String) =
            (chain, "owner".to_string(), "eucl".to_string());

        ALLOWANCES
            .save(
                &mut deps.storage,
                mixed_key.clone(),
                &Allowance {
                    amount: Uint128::new(300),
                    spender: spender.clone(),
                },
            )
            .unwrap();
        ALLOWANCES
            .save(
                &mut deps.storage,
                lower_key.clone(),
                &Allowance {
                    amount: Uint128::new(100),
                    spender: spender.clone(),
                },
            )
            .unwrap();

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        execute_normalize_balance_keys(deps.as_mut(), info, None, None).unwrap();

        assert!(ALLOWANCES.load(&deps.storage, mixed_key).is_err());
        let result = ALLOWANCES.load(&deps.storage, lower_key).unwrap();
        assert_eq!(result.amount, Uint128::new(300));
    }

    #[test]
    fn test_transfer_rejects_mixed_case_to() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let valid_user = CrossChainUser::new(
            ChainUid::create("cosmos".to_string()).unwrap(),
            "cosmos1sender".to_string(),
        );
        seed_balance(&mut deps, valid_user.clone(), "eucl", 100);

        let mixed_case_to = CrossChainUser::new(
            ChainUid::create("cosmos".to_string()).unwrap(),
            "Cosmos1MiXeD".to_string(),
        );

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let mut deps_mut = deps.as_mut();
        let err = execute_transfer(
            &mut deps_mut,
            info,
            ExecuteTransfer {
                amount: Uint128::new(50),
                token_id: "eucl".to_string(),
                sender: Some(valid_user),
                to: mixed_case_to,
                from: None,
                msg: None,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Address must be lowercase"));
    }

    #[test]
    fn test_approve_rejects_mixed_case_spender() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let owner = vsl_user("owner");
        let mixed_case_spender =
            CrossChainUser::new(ChainUid::vsl_chain_uid().unwrap(), "Spender".to_string());

        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let err = execute_approve(
            deps.as_mut(),
            info,
            ExecuteApprove {
                amount: Uint128::new(10),
                token_id: "eucl".to_string(),
                spender: mixed_case_spender,
                owner,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Address must be lowercase"));
    }

    #[test]
    fn test_normalize_balance_keys_pagination() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let chain = ChainUid::create("cosmos".to_string()).unwrap();
        let key1: (ChainUid, String, String) =
            (chain.clone(), "AAAA".to_string(), "eucl".to_string());
        let key2: (ChainUid, String, String) = (chain, "BBBB".to_string(), "eucl".to_string());

        BALANCES
            .save(&mut deps.storage, key1.clone(), &Uint128::new(10))
            .unwrap();
        BALANCES
            .save(&mut deps.storage, key2.clone(), &Uint128::new(20))
            .unwrap();

        // Process only 1 entry
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let res = execute_normalize_balance_keys(deps.as_mut(), info, None, Some(1)).unwrap();
        let norm: u32 = res
            .attributes
            .iter()
            .find(|a| a.key == "normalized_balances")
            .unwrap()
            .value
            .parse()
            .unwrap();
        assert_eq!(norm, 1);

        // key1 normalized, key2 still mixed
        assert!(BALANCES.load(&deps.storage, key1).is_err());
        assert!(BALANCES.load(&deps.storage, key2.clone()).is_ok());

        // Process remaining
        let router = router_addr_for(&deps);
        let info = message_info(&router, &[]);
        let res = execute_normalize_balance_keys(deps.as_mut(), info, None, Some(10)).unwrap();
        let norm: u32 = res
            .attributes
            .iter()
            .find(|a| a.key == "normalized_balances")
            .unwrap()
            .value
            .parse()
            .unwrap();
        assert_eq!(norm, 1);
        assert!(BALANCES.load(&deps.storage, key2).is_err());
    }
}
