use cosmwasm_std::{ensure, DepsMut, Env, MessageInfo, Response, Uint256};
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

    let token_decimals = token.token_type.get_decimals()?;
    match token.token_type {
        TokenType::Smart { .. } => {
            let validated_decimals = token.token_type.query_decimals(&deps.as_ref())?;
            ensure!(
                validated_decimals == token_decimals,
                ContractError::DecimalsMismatch {
                    expected: token_decimals as u32,
                    received: validated_decimals as u32,
                }
            );
        }
        TokenType::Native { .. } => {
            // We don't have a stable check yet for native tokens decimals as their metadata might not be stored on chain
        }
        TokenType::Voucher { .. } => {}
    };

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
            euclid::events::TxType::PoolCreation,
        ))
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
            euclid::events::TxType::PoolCreation,
        ))
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
    amount_in: Uint256,
    recipients: Vec<Recipient>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
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
        TokenType::Native { denom, .. } => {
            fund_manager.use_fund(amount_in, denom)?;
        }
        TokenType::Smart {
            contract_address, ..
        } => {
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
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_deposit_token")
        .add_submessages(msgs))
}

pub fn execute_transfer_voucher(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token_id: Token,
    amount: Uint256,
    from: Option<CrossChainUser>,
    recipients: Vec<Recipient>,
    cross_chain_config: CrossChainConfig,
) -> Result<Response, ContractError> {
    // The transfer amount should be greater than zero
    ensure!(!amount.is_zero(), ContractError::ZeroAssetAmount {});
    let state = STATE.load(deps.storage)?;
    for recipient in recipients.iter() {
        recipient.validate()?;
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
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "transfer_voucher")
        .add_submessage(withdraw_msg))
}
