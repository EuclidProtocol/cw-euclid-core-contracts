use cosmwasm_std::{
    ensure, from_json, to_json_binary, Binary, CosmosMsg, DepsMut, Env, Event, MessageInfo,
    Response, StdError, SubMsg, Uint128, WasmMsg,
};
use euclid::{chain::ChainUid, error::ContractError, msgs::router::ExecuteMsg};
use euclid_ibc::{ack::make_ack_fail, msg::ChainIbcExecuteMsg};

use crate::{
    reply::EVM_RECEIVE_REPLY_ID,
    state::{CHAIN_UID_TO_CHAIN, MOCK_RELAYER_ADDRESSES, PROCESSED_PACKET_SEQUENCE},
};

pub fn execute_evm_receive_packet(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    chain_uid: ChainUid,
    msg: Binary,
    sequence: u128,
    hash: String,
) -> Result<Response, ContractError> {
    ensure!(
        MOCK_RELAYER_ADDRESSES
            .load(deps.storage)?
            .contains(&info.sender.to_string()),
        ContractError::Unauthorized {}
    );

    let chain = CHAIN_UID_TO_CHAIN.load(deps.storage, chain_uid.clone())?;
    ensure!(chain.is_evm(), ContractError::Unauthorized {});

    let processed_sequence_key = PROCESSED_PACKET_SEQUENCE.key((chain_uid.clone(), sequence));
    ensure!(
        !processed_sequence_key.has(deps.storage),
        ContractError::Generic {
            err: "Processed sequence already exists".to_string()
        }
    );
    processed_sequence_key.save(deps.storage, &Uint128::from(env.block.height))?;
    let receive_packet_event = Event::new("euclid-hub-receive-packet")
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string());

    let write_acknowledge_event = Event::new("euclid-evm-write-acknowledgement")
        .add_attribute("msg", msg.to_string())
        .add_attribute("chain_uid", chain_uid.to_string())
        .add_attribute("sequence", sequence.to_string())
        .add_attribute("hash", hash.to_string());

    let internal_msg = ExecuteMsg::ReceivePacketInternalCallback {
        msg: msg.clone(),
        chain_uid: chain_uid.clone(),
    };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });

    let sub_msg = SubMsg::reply_always(internal_msg, EVM_RECEIVE_REPLY_ID);
    let msg: Result<ChainIbcExecuteMsg, StdError> = from_json(&msg);
    let tx_id = msg
        .map(|m| m.get_tx_id())
        .unwrap_or("tx_id_not_found".to_string());

    Ok(Response::new()
        .add_attribute("action", "evm-write-acknowledgement")
        .add_attribute("method", "evm_packet_receive")
        .add_attribute("tx_id", tx_id)
        .set_data(make_ack_fail("default_fail".to_string())?)
        .add_event(write_acknowledge_event)
        .add_event(receive_packet_event)
        .add_submessage(sub_msg))
}
