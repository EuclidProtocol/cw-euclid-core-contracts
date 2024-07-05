#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcPacketReceiveMsg,
    IbcReceiveResponse, Response, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    chain::ChainUid,
    error::ContractError,
    events::{tx_event, TxType},
    msgs::{
        escrow::ExecuteMsg as EscrowExecuteMsg,
        factory::{ExecuteMsg, RegisterFactoryResponse, ReleaseEscrowResponse},
    },
    token::Token,
};
use euclid_ibc::{
    ack::{make_ack_fail, AcknowledgementMsg},
    msg::HubIbcExecuteMsg,
};

use crate::{
    reply::IBC_RECEIVE_REPLY_ID,
    state::{HUB_CHANNEL, STATE, TOKEN_TO_ESCROW},
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    _deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    let internal_msg = ExecuteMsg::IbcCallbackReceive {
        receive_msg: msg.clone(),
    };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });
    let sub_msg = SubMsg::reply_always(internal_msg, IBC_RECEIVE_REPLY_ID);
    Ok(IbcReceiveResponse::new()
        .add_submessage(sub_msg)
        .add_attribute("ibc_ack", format!("{msg:?}"))
        .add_attribute("method", "ibc_packet_receive")
        .set_ack(make_ack_fail("deafult_fail".to_string())?))
}

pub fn ibc_receive_internal_call(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<Response, ContractError> {
    let router = msg.packet.src.port_id.replace("wasm.", "");

    // Ensure that channel is same as registered in the state
    let channel = msg.packet.dest.channel_id;
    ensure!(
        HUB_CHANNEL.load(deps.storage)? == channel,
        ContractError::Unauthorized {}
    );

    let msg: HubIbcExecuteMsg = from_json(msg.packet.data)?;
    match msg {
        HubIbcExecuteMsg::RegisterFactory { chain_uid, tx_id } => {
            execute_register_router(deps, env, router, channel, chain_uid, tx_id)
        }
        HubIbcExecuteMsg::ReleaseEscrow {
            amount,
            token,
            to_address,
            ..
        } => execute_release_escrow(deps, env, channel, amount, token, to_address),
    }
}

fn execute_register_router(
    deps: DepsMut,
    env: Env,
    router: String,
    channel: String,
    chain_uid: ChainUid,
    tx_id: String,
) -> Result<Response, ContractError> {
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
        .add_event(tx_event(&tx_id, &router, TxType::RegisterFactory))
        .add_attribute("method", "register_router")
        .add_attribute("router", router)
        .add_attribute("channel", channel)
        .set_data(ack))
}

fn execute_release_escrow(
    deps: DepsMut,
    env: Env,
    channel: String,
    amount: Uint128,
    token: Token,
    to_address: String,
) -> Result<Response, ContractError> {
    let withdraw_msg = EscrowExecuteMsg::Withdraw {
        recipient: deps.api.addr_validate(&to_address)?,
        amount,
    };

    let ack_msg = ReleaseEscrowResponse {
        factory_address: env.contract.address.to_string(),
        chain_id: env.block.chain_id,
        amount,
        token: token.clone(),
        to_address,
    };

    let ack = to_json_binary(&AcknowledgementMsg::Ok(ack_msg))?;

    // Get escrow address
    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, token.validate()?.to_owned())?
        .into_string();

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: escrow_address,
            msg: to_json_binary(&withdraw_msg)?,
            funds: vec![],
        }))
        .add_attribute("method", "release escrow")
        .add_attribute("channel", channel)
        .set_data(ack))
}
