use cosmwasm_std::{to_json_binary, DepsMut, Env, Response};
use euclid::{
    cross_chain_user::CrossChainUser,
    deposit::DepositTokenResponse,
    error::ContractError,
    events::{tx_event, TxType},
    msgs::{
        router::TokenDenom,
        virtual_balance::{
            msg::{
                ExecuteMint, ExecuteMsg as VirtualBalanceMsg, QueryMsg as VirtualBalanceQueryMsg,
            },
            GetTokenMetadataByDenomResponse,
        },
        vlp::base::{DeregisterDenomResponse, RegisterDenomResponse},
    },
    normalize::normalize_token_to_voucher,
    swap::TransferVoucherResponse,
    token::{TokenMetadata, TokenWithDenom},
    voucher::BalanceKey,
};

use euclid_ibc::{
    ack::AcknowledgementMsg,
    router_ibc::{
        RouterCrossChainDepositTokenExecuteMsg, RouterCrossChainTransferVoucherExecuteMsg,
    },
};

use crate::{execute::token::execute_transfer_voucher, state::VIRTUAL_BALANCE_CONTRACT};

pub fn ibc_execute_register_denom(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: TokenWithDenom,
    tx_id: String,
) -> Result<Response, ContractError> {
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    let token_metadata = TokenMetadata {
        token: token.token.clone(),
        chain_uid: sender.chain_uid.clone(),
        token_type: token.token_type.clone(),
        allowed: true,
    };
    let msg = VirtualBalanceMsg::RegisterTokenMetadata {
        token_metadata: token_metadata.clone(),
    };

    let ack: AcknowledgementMsg<RegisterDenomResponse> =
        AcknowledgementMsg::Ok(RegisterDenomResponse {});

    Ok(Response::new()
        .add_message(msg.to_wasm_msg(virtual_balance_address.to_string(), vec![])?)
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::RegisterDenom,
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
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    let msg = VirtualBalanceMsg::DeregisterTokenMetadata {
        token_id: token.token.to_string(),
        chain_uid: sender.chain_uid.clone(),
        token_type: token.token_type.clone(),
    };

    let ack: AcknowledgementMsg<DeregisterDenomResponse> =
        AcknowledgementMsg::Ok(DeregisterDenomResponse {});

    Ok(Response::new()
        .add_message(msg.to_wasm_msg(virtual_balance_address.to_string(), vec![])?)
        .add_event(tx_event(
            &tx_id,
            &sender.to_sender_string(),
            TxType::DeregisterDenom,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "execute_deregister_denom")
        .set_data(to_json_binary(&ack)?))
}

pub fn ibc_execute_deposit_token(
    deps: &mut DepsMut,
    env: Env,
    msg: RouterCrossChainDepositTokenExecuteMsg,
) -> Result<Response, ContractError> {
    let sender = msg.clone().sender;

    // Escrow balance is now managed by virtual_balance contract during mint

    let deposit_token_response = DepositTokenResponse {
        amount: msg.amount_in,
        token: msg.asset_in.token.clone(),
        sender: msg.sender.clone(),
    };
    let ack = AcknowledgementMsg::Ok(deposit_token_response.clone());

    // Load state to get virtual balance address
    let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

    let token_metadata = deps
        .querier
        .query_wasm_smart::<GetTokenMetadataByDenomResponse>(
            virtual_balance_address.to_string(),
            &VirtualBalanceQueryMsg::GetTokenMetadataByDenom {
                token_id: msg.asset_in.token.to_string(),
                chain_uid: msg.sender.chain_uid.clone(),
                token_type: msg.asset_in.token_type.clone(),
            },
        )?;

    // Send mint msg to virtual balance
    let mint_msg = VirtualBalanceMsg::Mint(ExecuteMint {
        amount: msg.amount_in.into(),
        balance_key: BalanceKey {
            cross_chain_user: msg.sender.clone(),
            token_id: msg.asset_in.token.to_string(),
        },
        token_type: msg.asset_in.token_type.clone(),
        token_source_chain_uid: msg.sender.chain_uid.clone(),
    })
    .to_wasm_msg(virtual_balance_address.to_string(), vec![])?;

    let response = Response::new()
        .add_message(mint_msg)
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
        );

    let expected_voucher = normalize_token_to_voucher(
        msg.amount_in.into(),
        token_metadata.metadata.token_type.get_decimals()?,
    )?;

    let transfer_response = execute_transfer_voucher(
        deps,
        env,
        sender,
        msg.asset_in.token,
        expected_voucher,
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
