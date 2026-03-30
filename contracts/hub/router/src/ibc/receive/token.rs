use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, Response, SubMsg, Uint128};
use euclid::{
    cross_chain_user::CrossChainUser,
    deposit::DepositTokenResponse,
    error::ContractError,
    events::{deregister_denom_event, register_denom_event, tx_event, TxType},
    interface::ContractInterface,
    msgs::{
        router::TokenDenom,
        virtual_balance::{interface as vb_interface, msg::ExecuteMint},
        vlp::base::{DeregisterDenomResponse, RegisterDenomResponse},
    },
    swap::TransferVoucherResponse,
    token::TokenWithDenom,
    voucher::BalanceKey,
};
use euclid_ibc::{
    ack::AcknowledgementMsg,
    router_ibc::{
        RouterCrossChainDepositTokenExecuteMsg, RouterCrossChainTransferVoucherExecuteMsg,
    },
};

use crate::{
    execute::token::execute_transfer_voucher,
    state::{ESCROW_BALANCES, TOKEN_DENOMS, VIRTUAL_BALANCE_CONTRACT},
};

pub fn ibc_execute_register_denom(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: TokenWithDenom,
    tx_id: String,
) -> Result<Response, ContractError> {
    token.token.validate()?;

    let mut token_denoms = TOKEN_DENOMS
        .load(deps.storage, token.token.clone())
        .unwrap_or_default();

    let token_exists = token_denoms
        .iter()
        .any(|denom| denom.chain_uid == sender.chain_uid && denom.token_type == token.token_type);

    ensure!(!token_exists, ContractError::TokenAlreadyExist {});

    token_denoms.push(TokenDenom {
        chain_uid: sender.chain_uid.clone(),
        token_type: token.token_type.clone(),
    });
    println!("Register Denom Token Key: {:?}", token.token);
    TOKEN_DENOMS.save(deps.storage, token.token.clone(), &token_denoms)?;

    let ack: AcknowledgementMsg<RegisterDenomResponse> =
        AcknowledgementMsg::Ok(RegisterDenomResponse {});

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::RegisterDenom,
        ))
        .add_event(register_denom_event(
            &token.token,
            &sender.chain_uid.to_string(),
            &token.token_type,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_register_denom")
        .set_data(to_json_binary(&ack)?))
}

pub fn ibc_execute_deregister_denom(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: TokenWithDenom,
    tx_id: String,
) -> Result<Response, ContractError> {
    let mut token_denoms = TOKEN_DENOMS
        .load(deps.storage, token.token.clone())
        .unwrap_or_default();

    let token_exists = token_denoms
        .iter()
        .any(|denom| denom.chain_uid == sender.chain_uid && denom.token_type == token.token_type);

    ensure!(token_exists, ContractError::AssetDoesNotExist {});

    // Remove the denom from list
    token_denoms.retain(|denom| {
        denom.chain_uid != sender.chain_uid || denom.token_type != token.token_type
    });

    TOKEN_DENOMS.save(deps.storage, token.token.clone(), &token_denoms)?;

    let ack: AcknowledgementMsg<DeregisterDenomResponse> =
        AcknowledgementMsg::Ok(DeregisterDenomResponse {});

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::DeregisterDenom,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_deregister_denom")
        .add_event(deregister_denom_event(
            &token.token,
            &sender.chain_uid.to_string(),
            &token.token_type,
        ))
        .set_data(to_json_binary(&ack)?))
}

pub fn ibc_execute_deposit_token(
    deps: &mut DepsMut,
    env: Env,
    msg: RouterCrossChainDepositTokenExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = msg.clone().sender;

    // Add token 1 in escrow balance
    let token_escrow_key = (msg.asset_in.token.to_string(), sender.chain_uid.clone());
    let token_escrow_balance = ESCROW_BALANCES
        .may_load(deps.storage, token_escrow_key.clone())?
        .unwrap_or(Uint128::zero());

    let new_escrow_balance = token_escrow_balance.checked_add(msg.amount_in)?;

    ESCROW_BALANCES.save(deps.storage, token_escrow_key, &new_escrow_balance)?;

    let deposit_token_response = DepositTokenResponse {
        amount: msg.amount_in,
        token: msg.asset_in.token.clone(),
        sender: msg.sender.clone(),
    };
    let ack = AcknowledgementMsg::Ok(deposit_token_response.clone());

    // Load state to get virtual balance address
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    // Send mint msg to virtual balance
    let mint_msg = vb_interface::MintMsg::Mint(ExecuteMint {
        amount: msg.amount_in,
        balance_key: BalanceKey {
            cross_chain_user: msg.sender.clone(),
            token_id: msg.asset_in.token.to_string(),
        },
    })
    .into_cosmos_msg(virtual_balance_address.to_string())?;

    let response = Response::new()
        .add_submessage(SubMsg::new(mint_msg))
        .add_attribute("action", "reply_deposit_token")
        .add_attribute(
            "deposit_token_response",
            format!("{deposit_token_response:?}"),
        )
        .add_event(
            tx_event(
                &msg.tx_id,
                &msg.sender.to_sender_string(),
                TxType::DepositToken,
            )
            .add_attribute("tx_id", msg.tx_id.clone()),
        )
        .add_attribute("chain_uid", sender.chain_uid.to_string())
        .add_attribute(
            format!(
                "escrow_added_token_{token}_denom_{denom}",
                token = msg.asset_in.token,
                denom = msg.asset_in.token_type.get_key()
            ),
            msg.amount_in,
        )
        .add_attribute(
            format!(
                "escrow_balance_token_{token}_denom_{denom}",
                token = msg.asset_in.token,
                denom = msg.asset_in.token_type.get_key()
            ),
            new_escrow_balance,
        );

    let transfer_response = execute_transfer_voucher(
        deps,
        env,
        sender,
        msg.asset_in.token,
        msg.amount_in,
        msg.recipients,
    )?;

    let response = response
        .add_submessages(transfer_response.messages)
        .add_attributes(transfer_response.attributes)
        .add_events(transfer_response.events);

    Ok(response.set_data(to_json_binary(&ack)?))
}

pub fn ibc_execute_transfer_virtual_balance(
    deps: &mut DepsMut,
    env: Env,
    msg: RouterCrossChainTransferVoucherExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = msg.clone().sender;

    let response = execute_transfer_voucher(
        deps,
        env,
        sender,
        msg.token.clone(),
        msg.amount,
        msg.recipients,
    )?;
    Ok(response
        .add_attribute("action", "transfer_virtual_balance")
        .add_event(
            tx_event(
                &msg.tx_id,
                &msg.sender.to_sender_string(),
                TxType::TransferVoucher,
            )
            .add_attribute("tx_id", msg.tx_id.clone()),
        )
        .set_data(to_json_binary(&AcknowledgementMsg::Ok(
            TransferVoucherResponse {
                token: msg.token,
                tx_id: msg.tx_id,
            },
        ))?))
}
