use cosmwasm_std::Uint128;
#[cfg(not(feature = "library"))]
use cosmwasm_std::{ensure, to_json_binary, DepsMut, Env, Response, SubMsg};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{tx_event, TxType},
    interface::ContractInterface,
    msgs::{escrow::interface as escrow_interface, factory::RegisterFactoryResponse, router::RegisterFactoryChainType},
    token::{Token, TokenType},
};
use euclid_ibc::{ack::AcknowledgementMsg, factory_ibc::FactoryCrossChainExecuteMsg};

use crate::{
    reply::RELEASE_ESCROW_REPLY_ID,
    state::{STATE, TOKEN_TO_ESCROW},
};

pub fn reusable_internal_call(
    deps: &mut DepsMut,
    env: Env,
    msg: FactoryCrossChainExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        FactoryCrossChainExecuteMsg::RegisterFactory {
            chain_uid,
            chain_type,
            tx_id,
        } => execute_register_router(deps.branch(), env, chain_uid, chain_type, tx_id),
        FactoryCrossChainExecuteMsg::ReleaseEscrow {
            sender,
            token,
            amount,
            denom,
            forwarding_message,
            tx_id,
            recipient,
        } => execute_release_escrow(
            deps.branch(),
            env,
            sender,
            token,
            amount,
            denom,
            forwarding_message,
            tx_id,
            recipient,
        ),
    }
}

fn execute_register_router(
    deps: DepsMut,
    env: Env,
    chain_uid: ChainUid,
    chain_type: RegisterFactoryChainType,
    tx_id: String,
) -> Result<Response, ContractError> {
    match chain_type {
        RegisterFactoryChainType::Cosmos(cosmos_info) => {
            ensure!(
                cosmos_info.factory_address == env.contract.address.to_string(),
                ContractError::new("Factory address mismatch")
            );
            ensure!(
                cosmos_info.factory_chain_id == env.block.chain_id,
                ContractError::new("Factory chain ID mismatch")
            );
        }
        RegisterFactoryChainType::Native(native_info) => {
            ensure!(
                native_info.factory_address == env.contract.address.to_string(),
                ContractError::new("Factory address mismatch")
            );
            ensure!(
                native_info.factory_chain_id == env.block.chain_id,
                ContractError::new("Factory chain ID mismatch")
            );
        }
        _ => {
            return Err(ContractError::new("Invalid chain type"));
        }
    }
    let chain_uid = chain_uid.validate()?.to_owned();
    let ack_msg = RegisterFactoryResponse {
        factory_address: env.contract.address.to_string(),
        chain_id: env.block.chain_id,
    };
    let state = STATE.load(deps.storage)?;

    ensure!(
        state.chain_uid == chain_uid,
        ContractError::new("Chain UID mismatch")
    );

    let ack = to_json_binary(&AcknowledgementMsg::Ok(ack_msg))?;

    Ok(Response::new()
        .add_event(tx_event(
            &tx_id,
            &state.router_contract,
            TxType::RegisterFactory,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "register_router")
        .add_attribute("router", state.router_contract)
        .set_data(ack))
}

fn execute_release_escrow(
    deps: DepsMut,
    _env: Env,
    sender: CrossChainUser,
    token: Token,
    amount: Uint128,
    denom: TokenType,
    forwarding_message: Option<String>,
    tx_id: String,
    recipient: String,
) -> Result<Response, ContractError> {
    // Get escrow address
    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, token.validate()?.to_owned())?
        .into_string();

    let response = Response::new();
    let recipient = deps.api.addr_validate(&recipient)?;

    let user_withdraw_msg = escrow_interface::WithdrawMsg::Withdraw {
        recipient: recipient.clone(),
        amount,
        denom,
        forwarding_message,
    }
    .into_cosmos_msg(escrow_address.clone())?;

    let user_withdraw_msg = SubMsg::reply_always(user_withdraw_msg, RELEASE_ESCROW_REPLY_ID);

    Ok(response
        .add_submessage(user_withdraw_msg)
        .add_attribute("method", "release escrow_execute")
        .add_attribute("sender", sender.to_sender_string())
        .add_attribute("token", token.to_string())
        .add_attribute("amount", amount.to_string())
        .add_attribute("tx_id", tx_id)
        .add_attribute("to_address", recipient))
}
