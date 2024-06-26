#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcPacketReceiveMsg,
    IbcReceiveResponse, Uint128, WasmMsg,
};
use euclid::{
    error::ContractError,
    msgs::{
        escrow::ExecuteMsg as EscrowExecuteMsg,
        factory::{RegisterFactoryResponse, ReleaseEscrowResponse},
    },
    token::Token,
};
use euclid_ibc::{
    ack::make_ack_fail,
    msg::{AcknowledgementMsg, HubIbcExecuteMsg},
};

use crate::state::{STATE, TOKEN_TO_ESCROW};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // Regardless of if our processing of this packet works we need to
    // commit an ACK to the chain. As such, we wrap all handling logic
    // in a seprate function and on error write out an error ack.
    match do_ibc_packet_receive(deps, env, msg) {
        Ok(response) => Ok(response),
        Err(error) => Ok(IbcReceiveResponse::new()
            .add_attribute("method", "ibc_packet_receive")
            .add_attribute("error", error.to_string())
            .set_ack(make_ack_fail(error.to_string())?)),
    }
}

pub fn do_ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // TODO: Check for channel with hub channel in state
    let channel = msg.packet.dest.channel_id;
    let msg: HubIbcExecuteMsg = from_json(msg.packet.data)?;
    match msg {
        HubIbcExecuteMsg::RegisterFactory { router } => {
            execute_register_router(deps, env, router, channel)
        }
        HubIbcExecuteMsg::ReleaseEscrow {
            amount,
            token_id,
            to_address,
            to_chain_id,
        } => execute_release_escrow(
            deps,
            env,
            channel,
            amount,
            token_id,
            to_address,
            to_chain_id,
        ),
    }
}

fn execute_register_router(
    deps: DepsMut,
    env: Env,
    router: String,
    channel: String,
) -> Result<IbcReceiveResponse, ContractError> {
    let mut state = STATE.load(deps.storage)?;
    ensure!(
        state.router_contract == router,
        ContractError::Unauthorized {}
    );
    state.hub_channel = Some(channel.clone());

    STATE.save(deps.storage, &state)?;

    let ack_msg = RegisterFactoryResponse {
        factory_address: env.contract.address.to_string(),
        chain_id: env.block.chain_id,
    };

    let ack = to_json_binary(&AcknowledgementMsg::Ok(ack_msg))?;

    Ok(IbcReceiveResponse::new()
        .add_attribute("method", "register_router")
        .add_attribute("router", router)
        .add_attribute("channel", channel)
        .set_ack(ack))
}

fn execute_release_escrow(
    deps: DepsMut,
    env: Env,
    channel: String,
    amount: Uint128,
    token_id: String,
    to_address: String,
    to_chain_id: String,
) -> Result<IbcReceiveResponse, ContractError> {
    let state = STATE.load(deps.storage)?;

    let withdraw_msg = EscrowExecuteMsg::Withdraw {
        recipient: to_address.clone(),
        amount,
        chain_id: state.chain_id,
    };

    let ack_msg = ReleaseEscrowResponse {
        factory_address: env.contract.address.to_string(),
        chain_id: env.block.chain_id,
        amount,
        token_id: token_id.clone(),
        to_address,
        to_chain_id,
    };

    let ack = to_json_binary(&AcknowledgementMsg::Ok(ack_msg))?;

    // Get escrow address
    let escrow_address = TOKEN_TO_ESCROW
        .load(deps.storage, Token { id: token_id })?
        .into_string();

    Ok(IbcReceiveResponse::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: escrow_address,
            msg: to_json_binary(&withdraw_msg)?,
            funds: vec![],
        }))
        .add_attribute("method", "release escrow")
        .add_attribute("channel", channel)
        .set_ack(ack))
}
