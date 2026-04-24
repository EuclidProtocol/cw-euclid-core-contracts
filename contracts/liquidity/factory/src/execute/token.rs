use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response, Uint128};
use euclid::{
    cross_chain_user::CrossChainUser,
    deposit::DepositTokenRequest,
    error::ContractError,
    events::{deposit_token_event, tx_event, TxType},
    msgs::{cross_chain_config::CrossChainConfig, escrow::AllowedTokenResponse},
    recipient::Recipient,
    token::{Token, TokenType, TokenWithDenom},
    utils::{fund_manager::FundManager, tx::generate_tx},
};
use euclid_ibc::router_ibc::{
    RouterCrossChainDepositTokenExecuteMsg, RouterCrossChainExecuteMsg,
    RouterCrossChainTransferVoucherExecuteMsg,
};

use crate::{
    query::get_chain_type,
    state::{
        DenomRegisterDeregisterRequest, ADMIN, PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS,
        PENDING_TOKEN_DEPOSIT, STATE, TOKEN_TO_ESCROW,
    },
};

pub fn execute_request_register_denom(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token: TokenWithDenom,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // Vouchers are not registered
    ensure!(
        !token.token_type.is_voucher(),
        ContractError::UnsupportedDenomination {}
    );

    let state = STATE.load(deps.storage)?;
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        admin.general_admin == info.sender,
        ContractError::Unauthorized {}
    );

    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
            .has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token.token.clone())?;
    if let Some(escrow_address) = escrow_address {
        let denom_allowed_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
            denom: token.token_type.clone(),
        };
        let denom_allowed: AllowedTokenResponse = deps
            .querier
            .query_wasm_smart(escrow_address, &denom_allowed_msg)?;

        // Denom should not be already registered
        ensure!(
            !denom_allowed.allowed,
            ContractError::EscrowAlreadyExists {}
        );
    }

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let request_register_denom_msg = RouterCrossChainExecuteMsg::RegisterDenom {
        token: token.clone(),
        sender,
        tx_id: tx_id.clone(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    let req = DenomRegisterDeregisterRequest {
        tx_id: tx_id.clone(),
        sender: info.sender.clone(),
        token: token.clone(),
    };

    PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &req,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::RegisterDenom,
        ))
        .add_attribute("action", "register_denom")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_register_denom")
        .add_attribute("token", token.token.to_string())
        .add_attribute("token_type", token.token_type.get_key())
        .add_submessage(request_register_denom_msg))
}

pub fn execute_request_deregister_denom(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token: TokenWithDenom,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // Vouchers are not registered
    ensure!(
        !token.token_type.is_voucher(),
        ContractError::UnsupportedDenomination {}
    );

    let state = STATE.load(deps.storage)?;
    let admin = ADMIN.load(deps.storage)?;
    ensure!(
        admin.general_admin == info.sender,
        ContractError::Unauthorized {}
    );

    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
            .has(deps.storage, (info.sender.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    let escrow_address = TOKEN_TO_ESCROW.load(deps.storage, token.token.clone())?;
    let denom_allowed_msg = euclid::msgs::escrow::QueryMsg::TokenAllowed {
        denom: token.token_type.clone(),
    };
    let denom_allowed: AllowedTokenResponse = deps
        .querier
        .query_wasm_smart(escrow_address, &denom_allowed_msg)?;

    // Denom should be allowed for it to be available for deregister
    ensure!(denom_allowed.allowed, ContractError::AssetDoesNotExist {});

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let request_deregister_denom_msg = RouterCrossChainExecuteMsg::DeregisterDenom {
        token: token.clone(),
        sender,
        tx_id: tx_id.clone(),
    }
    .to_msg(
        deps,
        &env,
        state.router_contract,
        info.sender.clone(),
        state.chain_uid,
        chain_type,
        cross_chain_config.timeout,
        cross_chain_config.ack_response,
    )?;

    let req = DenomRegisterDeregisterRequest {
        tx_id: tx_id.clone(),
        sender: info.sender.clone(),
        token: token.clone(),
    };

    PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS.save(
        deps.storage,
        (info.sender.clone(), tx_id.clone()),
        &req,
    )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            euclid::events::TxType::DeregisterDenom,
        ))
        .add_attribute("action", "deregister_denom")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "request_deregister_denom")
        .add_attribute("token", token.token.to_string())
        .add_attribute("token_type", token.token_type.get_key())
        .add_submessage(request_deregister_denom_msg))
}

pub fn execute_deposit_token(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    sender: CrossChainUser,
    asset_in: TokenWithDenom,
    amount_in: Uint128,
    recipients: Vec<Recipient>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    // Reject mixed-case or empty addresses before mutating state
    sender.validate()?;
    let sender_addr = deps.api.addr_validate(&sender.address)?;
    let state = STATE.load(deps.storage)?;
    ensure!(
        !asset_in.token_type.is_voucher(),
        ContractError::UnsupportedDenomination {}
    );

    for recipient in recipients.iter() {
        recipient.validate()?;
    }

    // Validate asset in
    asset_in.token.validate()?;
    asset_in.token_type.validate(&deps.as_ref())?;

    let tx_id = generate_tx(deps, &env, &sender)?;

    ensure!(
        !PENDING_TOKEN_DEPOSIT.has(deps.storage, (sender_addr.clone(), tx_id.clone())),
        ContractError::TxAlreadyExist {}
    );
    // Verify that this asset is allowed
    let escrow = TOKEN_TO_ESCROW.load(deps.storage, asset_in.token.clone())?;

    let token_allowed: euclid::msgs::escrow::AllowedTokenResponse = deps.querier.query_wasm_smart(
        escrow,
        &euclid::msgs::escrow::QueryMsg::TokenAllowed {
            denom: asset_in.token_type.clone(),
        },
    )?;
    ensure!(
        token_allowed.allowed,
        ContractError::UnsupportedDenomination {}
    );
    let mut fund_manager = FundManager::new(&info.funds);
    let mut msgs = Vec::new();

    match &asset_in.token_type {
        TokenType::Native { denom } => {
            fund_manager.use_fund(amount_in.into(), denom)?;
        }
        TokenType::Smart { contract_address } => {
            ensure!(
                info.sender.as_str() == contract_address,
                ContractError::Unauthorized {}
            );
        }
        TokenType::Voucher { .. } => {}
    }

    ensure!(
        fund_manager.validate_funds_are_empty().is_ok(),
        ContractError::new("Extra funds sent with message")
    );

    let deposit_token_info = DepositTokenRequest {
        sender: sender.address.to_string(),
        asset_in: asset_in.clone(),
        amount_in,
        tx_id: tx_id.clone(),
    };

    PENDING_TOKEN_DEPOSIT.save(
        deps.storage,
        (sender_addr.clone(), tx_id.clone()),
        &deposit_token_info,
    )?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;

    let asset_in_id = asset_in.token.to_string();

    let deposit_token_msg =
        RouterCrossChainExecuteMsg::DepositToken(RouterCrossChainDepositTokenExecuteMsg {
            sender,
            asset_in,
            amount_in,
            tx_id: tx_id.clone(),
            recipients,
        })
        .to_msg(
            deps,
            &env,
            state.clone().router_contract,
            sender_addr.clone(),
            state.clone().chain_uid,
            chain_type,
            cross_chain_config.timeout,
            cross_chain_config.ack_response,
        )?;
    msgs.push(deposit_token_msg);

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            sender_addr.as_str(),
            euclid::events::TxType::DepositToken,
        ))
        .add_event(deposit_token_event(&tx_id, &deposit_token_info))
        .add_attribute("action", "deposit_token")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_deposit_token")
        .add_attribute("asset_in", asset_in_id)
        .add_attribute("amount_in", amount_in)
        .add_submessages(msgs))
}

pub fn execute_transfer_voucher(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: Token,
    amount: Uint128,
    from: Option<CrossChainUser>,
    recipients: Vec<Recipient>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    cw_utils::nonpayable(&info)?;
    // The transfer amount should be greater than zero
    ensure!(!amount.is_zero(), ContractError::ZeroAssetAmount {});
    let state = STATE.load(deps.storage)?;
    for recipient in recipients.iter() {
        recipient.validate()?;
    }

    // Validate optional from address before sending cross-chain
    if let Some(ref from) = from {
        from.validate()?;
    }

    let sender = CrossChainUser::new(state.chain_uid.clone(), info.sender.to_string());
    let tx_id = generate_tx(deps, &env, &sender)?;

    let chain_type = get_chain_type(deps.as_ref(), &env)?;
    let withdraw_msg =
        RouterCrossChainExecuteMsg::TransferVoucher(RouterCrossChainTransferVoucherExecuteMsg {
            sender,
            token: token_id,
            amount,
            from,
            recipients,
            tx_id: tx_id.clone(),
        })
        .to_msg(
            deps,
            &env,
            state.router_contract,
            info.sender.clone(),
            state.chain_uid,
            chain_type,
            cross_chain_config.timeout,
            cross_chain_config.ack_response,
        )?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            info.sender.as_str(),
            TxType::TransferVoucher,
        ))
        .add_attribute("action", "transfer_voucher")
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "transfer_voucher")
        .add_submessage(withdraw_msg))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        testing::{message_info, mock_dependencies, mock_env},
        Uint128,
    };
    use euclid::{
        chain::ChainUid,
        error::ContractError,
        msgs::factory::ExecuteMsg,
        token::{Token, TokenType},
    };

    use crate::{
        contract::execute,
        testing::helpers::{
            assert_attribute, assert_tx_event_full, default_cross_chain_config, get_attribute,
            init, native_token, seed_escrow, set_escrow_token_allowed, voucher_token,
            TEST_CHAIN_UID,
        },
    };

    // -----------------------------------------------------------------------
    // Execute: RegisterDenom – authorization check
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_denom_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);
        deps.querier
            .bank
            .update_balance("addr", vec![cosmwasm_std::coin(1000, "uusdc")]);

        let non_admin = deps.api.addr_make("stranger");
        let info = message_info(&non_admin, &[]);
        let msg = ExecuteMsg::RegisterDenom {
            token_with_denom: native_token("usdc", "uusdc"),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_register_denom_voucher_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let admin = deps.api.addr_make("sender");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::RegisterDenom {
            token_with_denom: voucher_token("usdc"),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::UnsupportedDenomination {});
    }

    #[test]
    fn test_register_denom_happy_path_writes_pending_state() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);

        set_escrow_token_allowed(&mut deps, false);

        let token_with_denom = native_token("usdc", "uusdc");
        let admin = deps.api.addr_make("sender");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::RegisterDenom {
            token_with_denom: token_with_denom.clone(),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "request_register_denom"));

        let tx_id = get_attribute(&res, "tx_id").to_owned();
        assert!(!tx_id.is_empty());

        let pending = crate::state::PENDING_DENOM_REGISTER_DEREGISTER_REQUESTS
            .load(&deps.storage, (admin.clone(), tx_id.clone()))
            .unwrap();
        assert_eq!(pending.tx_id, tx_id);
        assert_eq!(pending.sender, admin);

        assert_attribute(&res, "action", "register_denom");
        assert_eq!(get_attribute(&res, "tx_id"), tx_id);
        assert_eq!(
            get_attribute(&res, "token"),
            token_with_denom.token.to_string()
        );
        assert_eq!(
            get_attribute(&res, "token_type"),
            token_with_denom.token_type.get_key()
        );
        assert_tx_event_full(&res, "register_denom", &tx_id, admin.as_str());
    }

    // -----------------------------------------------------------------------
    // Execute: TransferVoucher
    // -----------------------------------------------------------------------

    #[test]
    fn test_transfer_voucher_zero_amount_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let sender = deps.api.addr_make("sender");
        let recipient_user = euclid::cross_chain_user::CrossChainUser::new(
            ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
            deps.api.addr_make("recipient").to_string(),
        );
        let info = message_info(&sender, &[]);
        let msg = ExecuteMsg::TransferVoucher {
            token_id: Token::create("usdc".to_string()).unwrap(),
            amount: Uint128::zero(),
            from: None,
            recipients: vec![euclid::recipient::Recipient {
                recipient: recipient_user,
                amount: euclid::limit::Limit::LessThanOrEqual(Uint128::new(100)),
                denom: TokenType::Voucher {},
                forwarding_message: None,
                unsafe_refund_as_voucher: None,
            }],
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::ZeroAssetAmount {});
    }

    #[test]
    fn test_transfer_voucher_nonzero_amount_emits_attributes() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let token_id = Token::create("usdc".to_string()).unwrap();
        let amount = Uint128::new(500);

        let sender = deps.api.addr_make("sender");
        let recipient_user = euclid::cross_chain_user::CrossChainUser::new(
            ChainUid::create(TEST_CHAIN_UID.to_string()).unwrap(),
            deps.api.addr_make("recipient").to_string(),
        );
        let info = message_info(&sender, &[]);
        let msg = ExecuteMsg::TransferVoucher {
            token_id: token_id.clone(),
            amount,
            from: None,
            recipients: vec![euclid::recipient::Recipient {
                recipient: recipient_user,
                amount: euclid::limit::Limit::LessThanOrEqual(amount),
                denom: TokenType::Voucher {},
                forwarding_message: None,
                unsafe_refund_as_voucher: None,
            }],
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let tx_id = get_attribute(&res, "tx_id").to_owned();

        assert!(res
            .attributes
            .iter()
            .any(|a| a.key == "method" && a.value == "transfer_voucher"));

        assert_attribute(&res, "action", "transfer_voucher");
        assert_eq!(get_attribute(&res, "tx_id"), tx_id);
        assert_tx_event_full(&res, "transfer_voucher", &tx_id, sender.as_str());
    }

    // -----------------------------------------------------------------------
    // Execute: DeregisterDenom – authorization check
    // -----------------------------------------------------------------------

    #[test]
    fn test_deregister_denom_unauthorized() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000, "uusdc")]);

        let non_admin = deps.api.addr_make("stranger");
        let info = message_info(&non_admin, &[]);
        let msg = ExecuteMsg::DeregisterDenom {
            token_with_denom: native_token("usdc", "uusdc"),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::Unauthorized {});
    }

    #[test]
    fn test_deregister_denom_voucher_rejected() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        let admin = deps.api.addr_make("sender");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::DeregisterDenom {
            token_with_denom: voucher_token("usdc"),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::UnsupportedDenomination {});
    }

    #[test]
    fn test_deregister_denom_escrow_does_not_exist() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000, "uusdc")]);

        let admin = deps.api.addr_make("sender");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::DeregisterDenom {
            token_with_denom: native_token("usdc", "uusdc"),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert!(res.is_err());
    }

    // -----------------------------------------------------------------------
    // Execute: RegisterDenom – escrow already has the token allowed
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_denom_escrow_already_has_token_fails() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000_000, "uusdc")]);

        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        set_escrow_token_allowed(&mut deps, true);

        let admin = deps.api.addr_make("sender");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::RegisterDenom {
            token_with_denom: native_token("usdc", "uusdc"),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg);
        assert_eq!(res.unwrap_err(), ContractError::EscrowAlreadyExists {});
    }

    // -----------------------------------------------------------------------
    // Execute: DeregisterDenom – correct TxType in tx_event
    // -----------------------------------------------------------------------

    #[test]
    fn test_deregister_denom_happy_path_emits_correct_tx_type() {
        let mut deps = mock_dependencies();
        init(&mut deps);

        seed_escrow(&mut deps, "usdc", "escrow_usdc");
        set_escrow_token_allowed(&mut deps, true);

        deps.querier
            .bank
            .update_balance("any", vec![cosmwasm_std::coin(1_000, "uusdc")]);

        let token_with_denom = native_token("usdc", "uusdc");
        let admin = deps.api.addr_make("sender");
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::DeregisterDenom {
            token_with_denom: token_with_denom.clone(),
            cross_chain_config: default_cross_chain_config(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let tx_id = get_attribute(&res, "tx_id").to_owned();

        assert_attribute(&res, "action", "deregister_denom");
        assert_eq!(
            get_attribute(&res, "token"),
            token_with_denom.token.to_string()
        );
        assert_eq!(
            get_attribute(&res, "token_type"),
            token_with_denom.token_type.get_key()
        );
        assert_tx_event_full(&res, "deregister_denom", &tx_id, admin.as_str());
    }
}
