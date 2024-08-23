#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcPacketReceiveMsg,
    IbcReceiveResponse, Response, StdError, SubMsg, Uint128, WasmMsg,
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

    let msg: Result<HubIbcExecuteMsg, StdError> = from_json(&msg.packet.data);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    Ok(IbcReceiveResponse::new()
        .add_attribute("method", "ibc_packet_receive")
        .add_attribute("tx_id", tx_id)
        .set_ack(make_ack_fail("deafult_fail".to_string())?)
        .add_submessage(sub_msg))
}

pub fn ibc_receive_internal_call(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<Response, ContractError> {
    let router = msg.packet.src.port_id.replace("wasm.", "");

    let state = STATE.load(deps.storage)?;
    ensure!(
        state.router_contract == router,
        ContractError::Unauthorized {}
    );

    // Ensure that channel is same as registered in the state
    let channel = msg.packet.dest.channel_id;
    ensure!(
        HUB_CHANNEL.load(deps.storage)? == channel,
        ContractError::Unauthorized {}
    );

    let msg: HubIbcExecuteMsg = from_json(msg.packet.data)?;
    reusable_internal_call(deps, env, msg)
}

pub fn reusable_internal_call(
    deps: DepsMut,
    env: Env,
    msg: HubIbcExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        HubIbcExecuteMsg::RegisterFactory { chain_uid, tx_id } => {
            execute_register_router(deps, env, chain_uid, tx_id)
        }
        HubIbcExecuteMsg::ReleaseEscrow {
            amount,
            token,
            to_address,
            tx_id,
            ..
        } => execute_release_escrow(deps, env, amount, token, to_address, tx_id),
        HubIbcExecuteMsg::UpdateFactoryChannel { chain_uid, tx_id } => {
            execute_update_factory_channel(deps, env, chain_uid, tx_id)
        }
    }
}

fn execute_register_router(
    deps: DepsMut,
    env: Env,
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

fn execute_update_factory_channel(
    deps: DepsMut,
    env: Env,
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
        .add_event(tx_event(
            &tx_id,
            &state.router_contract,
            TxType::UpdateFactoryChannel,
        ))
        .add_attribute("tx_id", tx_id)
        .add_attribute("method", "update_factory_channel")
        .add_attribute("router", state.router_contract)
        .set_data(ack))
}
fn execute_release_escrow(
    deps: DepsMut,
    env: Env,
    amount: Uint128,
    token: Token,
    to_address: String,
    tx_id: String,
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
        to_address: to_address.clone(),
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
        .add_attribute("tx_id", tx_id)
        .add_attribute("to_address", to_address)
        .set_data(ack))
}
