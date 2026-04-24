use cosmwasm_std::{
    ensure, from_json, to_json_binary, Addr, Binary, DepsMut, Env, MessageInfo, Response, Uint128,
};

use euclid::cw20_types::Cw20ReceiveMsg;
use euclid::{
    error::ContractError,
    msgs::{escrow::cw20::EscrowCw20HookMsg, factory::ReleaseEscrowResponse, hook::EuclidReceive},
    token::TokenType,
};

use crate::state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT, DISALLOWED_DENOMS, STATE};

use euclid_ibc::ack::AcknowledgementMsg;

pub fn execute_add_allowed_denom(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    denom: TokenType,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // Vouchers are not escrowed
    ensure!(!denom.is_voucher(), ContractError::CannotEscrowVoucher {});

    // TODO nonpayable to this function? would be better to limit depositing funds through the deposit functions
    // Only the factory can call this function
    let factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );

    let mut allowed_denoms = ALLOWED_DENOMS.may_load(deps.storage)?.unwrap_or_default();

    // Make sure that the denom isn't already in the list
    ensure!(
        !allowed_denoms.contains(&denom),
        ContractError::DuplicateDenominations {}
    );
    allowed_denoms.push(denom.clone());

    ALLOWED_DENOMS.save(deps.storage, &allowed_denoms)?;

    // Remove from disallowed denoms if present
    let mut disallowed_denoms = DISALLOWED_DENOMS.load(deps.storage).unwrap_or_default();
    disallowed_denoms.retain(|d| d != &denom);
    DISALLOWED_DENOMS.save(deps.storage, &disallowed_denoms)?;

    // Add the new denom to denom to amount map
    let new_amount =
        DENOM_TO_AMOUNT.update(deps.storage, denom.get_key(), |existing| match existing {
            Some(existing) => Ok::<_, ContractError>(existing),
            None => Ok(Uint128::zero()),
        })?;

    Ok(Response::new()
        .add_attribute("method", "add_allowed_denom")
        .add_attribute("new_denom", denom.get_key())
        .add_attribute("amount", new_amount))
}

pub fn execute_disallow_denom(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    denom: TokenType,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // Only the factory can call this function
    let factory_address = STATE.load(deps.storage)?.factory_address;
    ensure!(
        info.sender == factory_address,
        ContractError::Unauthorized {}
    );

    let mut allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    // Make sure that the denom is already in the list
    ensure!(
        allowed_denoms.contains(&denom),
        ContractError::DenomDoesNotExist {}
    );
    // Remove denom from list
    allowed_denoms.retain(|current_denom| current_denom != &denom);
    ALLOWED_DENOMS.save(deps.storage, &allowed_denoms)?;

    // Add to disallowed denoms
    let mut disallowed_denoms = DISALLOWED_DENOMS.load(deps.storage).unwrap_or_default();
    disallowed_denoms.push(denom.clone());
    DISALLOWED_DENOMS.save(deps.storage, &disallowed_denoms)?;

    //TODO refund the disallowed funds
    Ok(Response::new()
        .add_attribute("method", "disallow_denom")
        .add_attribute("deregistered_denom", denom.get_key()))
}

pub fn execute_deposit_native(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // Make sure funds were sent
    ensure!(
        !info.funds.is_empty(),
        ContractError::InsufficientDeposit {}
    );

    // Only the factory can call this function
    let mut state = STATE.load(deps.storage)?;

    ensure!(
        info.sender == state.factory_address,
        ContractError::Unauthorized {}
    );

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    let mut response = Response::new().add_attribute("method", "deposit_native");

    for token in info.funds {
        // Check that the amount of token sent is not zero
        ensure!(
            !token.amount.is_zero(),
            ContractError::InsufficientDeposit {}
        );
        let token_type = TokenType::Native {
            denom: token.denom.clone(),
        };
        // Make sure token is part of allowed denoms
        ensure!(
            allowed_denoms.contains(&token_type),
            ContractError::UnsupportedDenomination {}
        );

        // Check current balance of denom
        let current_balance = DENOM_TO_AMOUNT.load(deps.storage, token_type.get_key())?;

        let token_amount = Uint128::try_from(token.amount)
            .map_err(|_| ContractError::new("Coin amount exceeds Uint128 max"))?;
        // Add the sent amount to current balance and save it
        DENOM_TO_AMOUNT.save(
            deps.storage,
            token_type.get_key(),
            &current_balance.checked_add(token_amount)?,
        )?;
        state.total_amount = state.total_amount.checked_add(token_amount)?;

        response = response
            .add_attribute("denom", token_type.get_key())
            .add_attribute("amount", token.amount.to_string());
    }

    STATE.save(deps.storage, &state)?;

    Ok(response)
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    match from_json(&cw20_msg.msg)? {
        EscrowCw20HookMsg::Deposit {} => {
            let factory_address = STATE.load(deps.storage)?.factory_address;
            // Only the factory can call this function
            let sender = cw20_msg.sender;
            ensure!(
                sender == factory_address.to_string(),
                ContractError::Unauthorized {}
            );

            let amount_sent = cw20_msg.amount;
            // TODO should this check be on the factory level? Or even before the factory
            ensure!(
                !amount_sent.is_zero(),
                ContractError::InsufficientDeposit {}
            );
            let asset_sent = info.sender.clone().into_string();
            let asset_sent = TokenType::Smart {
                contract_address: asset_sent,
            };

            execute_deposit_cw20(deps, env, info, amount_sent, asset_sent)
        }
    }
}

pub fn execute_deposit_cw20(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    amount: Uint128,
    denom: TokenType,
) -> Result<Response, ContractError> {
    ensure!(denom.is_smart(), ContractError::UnsupportedDenomination {});

    // Non-zero and unauthorized checks were made in receive_cw20

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;

    // Make sure token is part of allowed denoms
    ensure!(
        allowed_denoms.contains(&denom),
        ContractError::UnsupportedDenomination {}
    );

    // Check current balance of denom
    let current_balance = DENOM_TO_AMOUNT.load(deps.storage, denom.get_key())?;

    // Add the sent amount to current balance and save it
    DENOM_TO_AMOUNT.save(
        deps.storage,
        denom.get_key(),
        &current_balance.checked_add(amount)?,
    )?;

    // Only the factory can call this function
    let mut state = STATE.load(deps.storage)?;
    state.total_amount = state.total_amount.checked_add(amount)?;

    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "deposit_cw20")
        .add_attribute("asset", denom.get_key())
        .add_attribute("amount", amount))
}

pub fn execute_withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: Addr,
    amount: Uint128,
    denom: TokenType,
    forwarding_message: Option<String>,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // Only the factory can call this function
    let mut state = STATE.load(deps.storage)?;
    // Only factory can trigger a withdraw
    ensure!(
        info.sender == state.factory_address,
        ContractError::Unauthorized {}
    );

    // Ensure that the amount desired is above zero
    ensure!(!amount.is_zero(), ContractError::ZeroWithdrawalAmount {});

    let allowed_denoms = ALLOWED_DENOMS.load(deps.storage)?;
    let disallowed_denoms = DISALLOWED_DENOMS.load(deps.storage).unwrap_or_default();

    let mut all_denoms = allowed_denoms;
    all_denoms.extend(disallowed_denoms);

    ensure!(
        all_denoms.iter().any(|d| d.get_key() == denom.get_key()),
        ContractError::UnsupportedDenomination {}
    );

    let denom_balance = DENOM_TO_AMOUNT.load(deps.storage, denom.get_key())?;

    // Ensure escrow has enough funds
    ensure!(
        denom_balance.ge(&amount),
        ContractError::InsufficientFunds {}
    );

    // Update denom balance state
    let new_balance = denom_balance.checked_sub(amount)?;
    DENOM_TO_AMOUNT.save(deps.storage, denom.get_key(), &new_balance)?;

    // Update total balance state
    state.total_amount = state.total_amount.checked_sub(amount)?;
    STATE.save(deps.storage, &state)?;

    // Wrap the forwading message into EuclidReceive Cosmos Msg
    let forwarding_message = match &forwarding_message {
        Some(forwarding_msg) => {
            let forwarding_msg = Binary::from_base64(forwarding_msg.as_str())?;
            Some(EuclidReceive::from_msg(forwarding_msg).to_receiver_msg()?)
        }
        None => None,
    };
    let send_msg = denom.create_transfer_msg(
        amount, // Transfer amount to recipient
        recipient.to_string(),
        None,
        forwarding_message.clone(),
    )?;

    let ack_msg = ReleaseEscrowResponse {
        amount,
        to_address: recipient.to_string(),
        escrow_balance: denom_balance,
    };
    let ack = to_json_binary(&AcknowledgementMsg::Ok(ack_msg))?;

    let response = Response::new()
        .add_message(send_msg)
        .add_attribute("method", "escrow_withdraw")
        .add_attribute("amount", amount)
        .add_attribute("token", state.token_id.to_string())
        .add_attribute("denom", denom.get_key())
        .add_attribute("recipient", recipient)
        .set_data(ack);

    Ok(response)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        attr, coin, from_json,
        testing::{message_info, mock_env},
        Addr, BankMsg, Binary, CosmosMsg, Uint128, WasmMsg,
    };
    use euclid::{
        error::ContractError,
        msgs::{escrow::ExecuteMsg, factory::ReleaseEscrowResponse},
        token::TokenType,
    };
    use euclid_ibc::ack::AcknowledgementMsg;
    use rstest::rstest;

    use crate::{
        contract::execute,
        state::{ALLOWED_DENOMS, DENOM_TO_AMOUNT, DISALLOWED_DENOMS, STATE},
        testing::{
            fixtures::{initialized, with_deposit},
            helpers::{
                init, make_cw20_receive_msg, native_denom, smart_denom, MockDeps, NATIVE_DENOM,
                TOKEN_ID,
            },
        },
    };

    // -----------------------------------------------------------------------
    // AddAllowedDenom
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_add_allowed_denom_happy_path(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let new_denom = TokenType::Native {
            denom: "uosmo".to_string(),
        };
        let info = message_info(&factory, &[]);
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::AddAllowedDenom {
                denom: new_denom.clone(),
            },
        )
        .unwrap();

        assert_eq!(res.attributes[0], attr("method", "add_allowed_denom"));
        assert_eq!(res.attributes[1], attr("new_denom", new_denom.get_key()));

        let allowed = ALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(allowed.contains(&new_denom));

        let bal = DENOM_TO_AMOUNT
            .load(&initialized.storage, new_denom.get_key())
            .unwrap();
        assert_eq!(bal, Uint128::zero());
    }

    #[rstest]
    fn test_add_allowed_denom_unauthorized(mut initialized: MockDeps) {
        let stranger = initialized.api.addr_make("stranger");
        let info = message_info(&stranger, &[]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::AddAllowedDenom {
                denom: native_denom(),
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[rstest]
    fn test_add_allowed_denom_duplicate_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::AddAllowedDenom {
                denom: native_denom(),
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::DuplicateDenominations {});
    }

    #[rstest]
    fn test_add_allowed_denom_voucher_rejected(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::AddAllowedDenom {
                denom: TokenType::Voucher {},
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::CannotEscrowVoucher {});
    }

    #[rstest]
    fn test_add_allowed_denom_removes_from_disallowed(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let factory_info = message_info(&factory, &[]);

        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info.clone(),
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let disallowed = DISALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(disallowed.contains(&native_denom()));

        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info.clone(),
            ExecuteMsg::AddAllowedDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let disallowed = DISALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(!disallowed.contains(&native_denom()));

        let allowed = ALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(allowed.contains(&native_denom()));
    }

    #[rstest]
    fn test_add_smart_denom_happy_path(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let cw20_addr = initialized.api.addr_make("cw20token");
        let denom = smart_denom(cw20_addr.as_str());
        let info = message_info(&factory, &[]);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::AddAllowedDenom {
                denom: denom.clone(),
            },
        )
        .unwrap();

        assert_eq!(res.attributes[0], attr("method", "add_allowed_denom"));

        let allowed = ALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(allowed.contains(&denom));
    }

    // -----------------------------------------------------------------------
    // DisallowDenom
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_disallow_denom_happy_path(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        assert_eq!(res.attributes[0], attr("method", "disallow_denom"));
        assert_eq!(
            res.attributes[1],
            attr("deregistered_denom", native_denom().get_key())
        );

        let allowed = ALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(!allowed.contains(&native_denom()));

        let disallowed = DISALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(disallowed.contains(&native_denom()));
    }

    #[rstest]
    fn test_disallow_denom_unauthorized(mut initialized: MockDeps) {
        let stranger = initialized.api.addr_make("stranger");
        let info = message_info(&stranger, &[]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[rstest]
    fn test_disallow_denom_not_in_allowed_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DisallowDenom {
                denom: TokenType::Native {
                    denom: "nonexistent".to_string(),
                },
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::DenomDoesNotExist {});
    }

    #[rstest]
    fn test_disallow_already_disallowed_denom_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let factory_info = message_info(&factory, &[]);

        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info.clone(),
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let err = execute(
            initialized.as_mut(),
            mock_env(),
            factory_info,
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::DenomDoesNotExist {});
    }

    #[rstest]
    fn test_disallow_multiple_denoms(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let factory_info = message_info(&factory, &[]);

        let denom2 = TokenType::Native {
            denom: "uatom".to_string(),
        };
        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info.clone(),
            ExecuteMsg::AddAllowedDenom {
                denom: denom2.clone(),
            },
        )
        .unwrap();

        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info.clone(),
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();
        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info.clone(),
            ExecuteMsg::DisallowDenom {
                denom: denom2.clone(),
            },
        )
        .unwrap();

        let allowed = ALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert!(allowed.is_empty());

        let disallowed = DISALLOWED_DENOMS.load(&initialized.storage).unwrap();
        assert_eq!(disallowed.len(), 2);
        assert!(disallowed.contains(&native_denom()));
        assert!(disallowed.contains(&denom2));
    }

    // -----------------------------------------------------------------------
    // DepositNative
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_deposit_native_happy_path(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[coin(500, NATIVE_DENOM)]);
        let res = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap();

        assert_eq!(res.attributes[0], attr("method", "deposit_native"));
        assert_eq!(res.attributes[1], attr("denom", native_denom().get_key()));
        assert_eq!(res.attributes[2], attr("amount", "500"));

        let bal = DENOM_TO_AMOUNT
            .load(&initialized.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::new(500));

        let state = STATE.load(&initialized.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(500));
    }

    #[rstest]
    fn test_deposit_native_accumulates(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");

        for _ in 0..3 {
            let info = message_info(&factory, &[coin(100, NATIVE_DENOM)]);
            execute(
                initialized.as_mut(),
                mock_env(),
                info,
                ExecuteMsg::DepositNative {},
            )
            .unwrap();
        }

        let bal = DENOM_TO_AMOUNT
            .load(&initialized.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::new(300));

        let state = STATE.load(&initialized.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(300));
    }

    #[rstest]
    fn test_deposit_native_no_funds_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::InsufficientDeposit {});
    }

    #[rstest]
    fn test_deposit_native_zero_amount_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[coin(0, NATIVE_DENOM)]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::InsufficientDeposit {});
    }

    #[rstest]
    fn test_deposit_native_unauthorized_sender(mut initialized: MockDeps) {
        let stranger = initialized.api.addr_make("stranger");
        let info = message_info(&stranger, &[coin(100, NATIVE_DENOM)]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[rstest]
    fn test_deposit_native_unsupported_denom_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let info = message_info(&factory, &[coin(100, "uatom")]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::UnsupportedDenomination {});
    }

    #[rstest]
    fn test_deposit_disallowed_denom_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let factory_info = message_info(&factory, &[]);

        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info,
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let info = message_info(&factory, &[coin(100, NATIVE_DENOM)]);
        let err = execute(
            initialized.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap_err();
        assert_eq!(err, ContractError::UnsupportedDenomination {});
    }

    // -----------------------------------------------------------------------
    // Receive (CW20 deposit)
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_receive_cw20_happy_path(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let cw20_contract = initialized.api.addr_make("cw20token");

        let factory_info = message_info(&factory, &[]);
        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info,
            ExecuteMsg::AddAllowedDenom {
                denom: smart_denom(cw20_contract.as_str()),
            },
        )
        .unwrap();

        let cw20_info = message_info(&cw20_contract, &[]);
        let recv_msg = make_cw20_receive_msg(factory.as_str(), 250);

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            cw20_info,
            ExecuteMsg::Receive(recv_msg),
        )
        .unwrap();

        assert_eq!(res.attributes[0], attr("method", "deposit_cw20"));

        let bal = DENOM_TO_AMOUNT
            .load(
                &initialized.storage,
                smart_denom(cw20_contract.as_str()).get_key(),
            )
            .unwrap();
        assert_eq!(bal, Uint128::new(250));

        let state = STATE.load(&initialized.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(250));
    }

    #[rstest]
    fn test_receive_cw20_unauthorized_inner_sender(mut initialized: MockDeps) {
        let cw20_contract = initialized.api.addr_make("cw20token");
        let factory = initialized.api.addr_make("factory");
        let stranger = initialized.api.addr_make("stranger");

        let factory_info = message_info(&factory, &[]);
        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info,
            ExecuteMsg::AddAllowedDenom {
                denom: smart_denom(cw20_contract.as_str()),
            },
        )
        .unwrap();

        let cw20_info = message_info(&cw20_contract, &[]);
        let recv_msg = make_cw20_receive_msg(stranger.as_str(), 100);

        let err = execute(
            initialized.as_mut(),
            mock_env(),
            cw20_info,
            ExecuteMsg::Receive(recv_msg),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[rstest]
    fn test_receive_cw20_zero_amount_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let cw20_contract = initialized.api.addr_make("cw20token");

        let factory_info = message_info(&factory, &[]);
        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info,
            ExecuteMsg::AddAllowedDenom {
                denom: smart_denom(cw20_contract.as_str()),
            },
        )
        .unwrap();

        let cw20_info = message_info(&cw20_contract, &[]);
        let recv_msg = make_cw20_receive_msg(factory.as_str(), 0);

        let err = execute(
            initialized.as_mut(),
            mock_env(),
            cw20_info,
            ExecuteMsg::Receive(recv_msg),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::InsufficientDeposit {});
    }

    #[rstest]
    fn test_receive_cw20_unsupported_contract_fails(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");
        let cw20_contract = initialized.api.addr_make("unknown_cw20");

        let cw20_info = message_info(&cw20_contract, &[]);
        let recv_msg = make_cw20_receive_msg(factory.as_str(), 100);

        let err = execute(
            initialized.as_mut(),
            mock_env(),
            cw20_info,
            ExecuteMsg::Receive(recv_msg),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::UnsupportedDenomination {});
    }

    // -----------------------------------------------------------------------
    // Withdraw
    // -----------------------------------------------------------------------

    #[rstest]
    fn test_withdraw_native_happy_path(mut with_deposit: MockDeps) {
        let factory = with_deposit.api.addr_make("factory");
        let recipient = Addr::unchecked("recipient");
        let info = message_info(&factory, &[]);

        let res = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: recipient.clone(),
                amount: Uint128::new(400),
                denom: native_denom(),
                forwarding_message: None,
            },
        )
        .unwrap();

        assert_eq!(res.attributes[0], attr("method", "escrow_withdraw"));
        assert_eq!(res.attributes[1], attr("amount", "400"));
        assert_eq!(res.attributes[2], attr("token", TOKEN_ID));
        assert_eq!(res.attributes[3], attr("denom", native_denom().get_key()));
        assert_eq!(res.attributes[4], attr("recipient", recipient.as_str()));

        assert_eq!(res.messages.len(), 1);
        if let CosmosMsg::Bank(BankMsg::Send { to_address, amount }) = &res.messages[0].msg {
            assert_eq!(to_address, recipient.as_str());
            assert_eq!(amount[0].denom, NATIVE_DENOM);
            assert_eq!(amount[0].amount, cosmwasm_std::Uint256::from(400u128));
        } else {
            panic!("expected BankMsg::Send");
        }

        let bal = DENOM_TO_AMOUNT
            .load(&with_deposit.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::new(600));

        let state = STATE.load(&with_deposit.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(600));
    }

    #[rstest]
    fn test_withdraw_response_data_contains_ack(mut with_deposit: MockDeps) {
        let factory = with_deposit.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let res = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(100),
                denom: native_denom(),
                forwarding_message: None,
            },
        )
        .unwrap();

        let data = res.data.expect("expected response data");
        let ack: AcknowledgementMsg<ReleaseEscrowResponse> = from_json(data).unwrap();
        match ack {
            AcknowledgementMsg::Ok(inner) => {
                assert_eq!(inner.amount, Uint128::new(100));
                assert_eq!(inner.to_address, "recip");
                assert_eq!(inner.escrow_balance, Uint128::new(1_000));
            }
            AcknowledgementMsg::Error(_) => panic!("expected Ok ack"),
        }
    }

    #[rstest]
    fn test_withdraw_unauthorized(mut with_deposit: MockDeps) {
        let stranger = with_deposit.api.addr_make("stranger");
        let info = message_info(&stranger, &[]);
        let err = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(100),
                denom: native_denom(),
                forwarding_message: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[rstest]
    fn test_withdraw_zero_amount_fails(mut with_deposit: MockDeps) {
        let factory = with_deposit.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let err = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::zero(),
                denom: native_denom(),
                forwarding_message: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::ZeroWithdrawalAmount {});
    }

    #[rstest]
    fn test_withdraw_insufficient_funds_fails(mut with_deposit: MockDeps) {
        let factory = with_deposit.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let err = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(9_999_999),
                denom: native_denom(),
                forwarding_message: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::InsufficientFunds {});
    }

    #[rstest]
    fn test_withdraw_unsupported_denom_fails(mut with_deposit: MockDeps) {
        let factory = with_deposit.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let unknown_denom = TokenType::Native {
            denom: "unknown".to_string(),
        };
        let err = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(100),
                denom: unknown_denom,
                forwarding_message: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::UnsupportedDenomination {});
    }

    #[rstest]
    fn test_withdraw_disallowed_denom_succeeds(mut initialized: MockDeps) {
        let factory = initialized.api.addr_make("factory");

        let deposit_info = message_info(&factory, &[coin(1_000, NATIVE_DENOM)]);
        execute(
            initialized.as_mut(),
            mock_env(),
            deposit_info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap();

        let factory_info = message_info(&factory, &[]);
        execute(
            initialized.as_mut(),
            mock_env(),
            factory_info.clone(),
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let res = execute(
            initialized.as_mut(),
            mock_env(),
            factory_info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(600),
                denom: native_denom(),
                forwarding_message: None,
            },
        );
        assert!(res.is_ok());

        let bal = DENOM_TO_AMOUNT
            .load(&initialized.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::new(400));

        let state = STATE.load(&initialized.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(400));
    }

    #[rstest]
    fn test_withdraw_exact_balance_succeeds(mut with_deposit: MockDeps) {
        let factory = with_deposit.api.addr_make("factory");
        let info = message_info(&factory, &[]);

        let res = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: Addr::unchecked("recip"),
                amount: Uint128::new(1_000),
                denom: native_denom(),
                forwarding_message: None,
            },
        );
        assert!(res.is_ok());

        let bal = DENOM_TO_AMOUNT
            .load(&with_deposit.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::zero());

        let state = STATE.load(&with_deposit.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::zero());
    }

    #[rstest]
    fn test_withdraw_with_forwarding_message_sends_wasm_execute(mut with_deposit: MockDeps) {
        let factory = with_deposit.api.addr_make("factory");
        let info = message_info(&factory, &[]);
        let recipient = Addr::unchecked("some_contract");

        let inner_binary = Binary::from(b"forwarding_payload".as_slice());
        let fwd_msg_b64 = inner_binary.to_base64();

        let res = execute(
            with_deposit.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::Withdraw {
                recipient: recipient.clone(),
                amount: Uint128::new(100),
                denom: native_denom(),
                forwarding_message: Some(fwd_msg_b64),
            },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 1);
        match &res.messages[0].msg {
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr,
                funds,
                ..
            }) => {
                assert_eq!(contract_addr, recipient.as_str());
                assert_eq!(funds[0].denom, NATIVE_DENOM);
                assert_eq!(funds[0].amount, cosmwasm_std::Uint256::from(100u128));
            }
            other => panic!("unexpected message type: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // CW20 + native balance independence
    // -----------------------------------------------------------------------

    #[test]
    fn test_cw20_and_native_deposits_tracked_independently() {
        use cosmwasm_std::testing::mock_dependencies;
        let mut deps = mock_dependencies();
        init(&mut deps);

        let factory = deps.api.addr_make("factory");
        let cw20_contract = deps.api.addr_make("cw20token");

        let finfo = message_info(&factory, &[]);
        execute(
            deps.as_mut(),
            mock_env(),
            finfo,
            ExecuteMsg::AddAllowedDenom {
                denom: smart_denom(cw20_contract.as_str()),
            },
        )
        .unwrap();

        let info = message_info(&factory, &[coin(700, NATIVE_DENOM)]);
        execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap();

        let cw20_info = message_info(&cw20_contract, &[]);
        let recv_msg = make_cw20_receive_msg(factory.as_str(), 300);
        execute(
            deps.as_mut(),
            mock_env(),
            cw20_info,
            ExecuteMsg::Receive(recv_msg),
        )
        .unwrap();

        let native_bal = DENOM_TO_AMOUNT
            .load(&deps.storage, native_denom().get_key())
            .unwrap();
        let smart_bal = DENOM_TO_AMOUNT
            .load(&deps.storage, smart_denom(cw20_contract.as_str()).get_key())
            .unwrap();
        assert_eq!(native_bal, Uint128::new(700));
        assert_eq!(smart_bal, Uint128::new(300));

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(1_000));
    }

    #[test]
    fn test_re_allow_denom_preserves_balance() {
        use cosmwasm_std::testing::mock_dependencies;
        let mut deps = mock_dependencies();
        init(&mut deps);
        let factory = deps.api.addr_make("factory");

        let info = message_info(&factory, &[coin(200, NATIVE_DENOM)]);
        execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap();

        let finfo = message_info(&factory, &[]);
        execute(
            deps.as_mut(),
            mock_env(),
            finfo.clone(),
            ExecuteMsg::DisallowDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        execute(
            deps.as_mut(),
            mock_env(),
            finfo.clone(),
            ExecuteMsg::AddAllowedDenom {
                denom: native_denom(),
            },
        )
        .unwrap();

        let info = message_info(&factory, &[coin(50, NATIVE_DENOM)]);
        execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::DepositNative {},
        )
        .unwrap();

        let bal = DENOM_TO_AMOUNT
            .load(&deps.storage, native_denom().get_key())
            .unwrap();
        assert_eq!(bal, Uint128::new(250));

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(state.total_amount, Uint128::new(250));
    }
}
