use crate::{
    ibc,
    state::{PENDING_DEPOSIT_TOKEN, TOKEN_TO_ESCROW, VLP_TO_CW20},
};
use cosmwasm_std::{from_json, DepsMut, Env, Event, Reply, Response, SubMsgResult};
use cw_utils::{parse_execute_response_data, parse_instantiate_response_data};
use euclid::{error::ContractError, events::simple_event};
use euclid_ibc::{ack::make_ack_fail, msg::CHAIN_IBC_EXECUTE_MSG_QUEUE};

pub const ESCROW_INSTANTIATE_REPLY_ID: u64 = 1;
pub const IBC_ACK_AND_TIMEOUT_REPLY_ID: u64 = 2;
pub const IBC_RECEIVE_REPLY_ID: u64 = 3;
pub const CW20_INSTANTIATE_REPLY_ID: u64 = 4;
pub const RELEASE_ESCROW_REPLY_ID: u64 = 5;

pub const COSMOS_RECEIVE_REPLY_ID: u64 = 6;
pub const COSMOS_ACK_AND_TIMEOUT_REPLY_ID: u64 = 7;

pub fn on_escrow_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let instantiate_data: cw_utils::MsgInstantiateContractResponse =
                parse_instantiate_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let escrow_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let escrow_data: euclid::msgs::escrow::EscrowInstantiateResponse =
                from_json(instantiate_data.data.unwrap_or_default())?;

            TOKEN_TO_ESCROW.save(deps.storage, escrow_data.token.clone(), &escrow_address)?;

            let mut response = Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("escrow", escrow_address.clone())
                .add_attribute("token_id", escrow_data.token.to_string());

            let pending_deposit_token =
                PENDING_DEPOSIT_TOKEN.may_load(deps.storage, escrow_data.token.clone())?;

            if let Some(token) = pending_deposit_token {
                let deposit_msg = token
                    .token_type
                    .create_escrow_msg(token.amount, escrow_address)?;
                response = response.add_message(deposit_msg);
                PENDING_DEPOSIT_TOKEN.remove(deps.storage, token.token);
            }

            Ok(response)
        }
    }
}

pub fn on_cw20_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let instantiate_data: cw_utils::MsgInstantiateContractResponse =
                parse_instantiate_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let cw20_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let cw20_data: euclid::msgs::escrow::Cw20InstantiateResponse =
                from_json(instantiate_data.data.unwrap_or_default())?;

            VLP_TO_CW20.save(deps.storage, cw20_data.vlp, &cw20_address)?;

            Ok(Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("cw20", cw20_address))
        }
    }
}

pub fn on_ibc_ack_and_timeout_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Ok(Response::new()
            .add_attribute("reply_on_ibc_ack_or_timeout_processing", "error")
            .add_attribute("error", err)),
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            Ok(Response::new()
                .add_attribute("reply_on_ibc_ack_or_timeout_processing", "success")
                .set_data(data))
        }
    }
}

pub fn on_ibc_receive_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Ok(Response::new()
            .add_attribute("reply_on_ibc_receive_processing", "error")
            .add_attribute("error", err.clone())
            .set_data(make_ack_fail(err)?)),
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            Ok(Response::new()
                .add_attribute("reply_on_ibc_receive_processing", "success")
                .set_data(data))
        }
    }
}

pub fn on_reply_native_ibc_wrapper_call(
    deps: DepsMut,
    env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    let original_msg = CHAIN_IBC_EXECUTE_MSG_QUEUE.load(deps.storage, msg.id)?;
    CHAIN_IBC_EXECUTE_MSG_QUEUE.remove(deps.storage, msg.id);
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let ack = make_ack_fail(err.clone())?;
            let response = ibc::ack_and_timeout::reusable_internal_ack_call(
                deps,
                env,
                original_msg,
                ack,
                true,
            )?;
            Ok(response
                .add_attribute("reply_on_ibc_receive_processing", "err")
                .add_attribute("err", err))
        }
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            let response = ibc::ack_and_timeout::reusable_internal_ack_call(
                deps,
                env,
                original_msg,
                data,
                true,
            )?;
            Ok(response.add_attribute("reply_on_ibc_receive_processing", "success"))
        }
    }
}

pub fn on_release_escrow_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic {
            err: err.to_string(),
        }),
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            Ok(Response::new()
                .add_attribute("reply_on_release_escrow_processing", "success")
                .set_data(data))
        }
    }
}

pub fn on_cosmos_receive_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let euclid_event = simple_event().add_attribute("action", "cosmos-relay");

            let write_acknowledge_event = Event::new("euclid-cosmos-write-acknowledgement")
                .add_attribute("ack", make_ack_fail(err.clone())?.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_cosmos_receive_processing", "error")
                .add_attribute("error", err.clone())
                .add_event(euclid_event)
                .add_event(write_acknowledge_event))
        }
        SubMsgResult::Ok(res) => {
            #[allow(deprecated)]
            let data = res
                .data
                .map(|data| {
                    parse_execute_response_data(&data)
                        .map(|d| d.data.unwrap_or_default())
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            let euclid_event =
                simple_event().add_attribute("action", "cosmos-write-acknowledgement");

            let write_acknowledge_event = Event::new("euclid-cosmos-write-acknowledgement")
                .add_attribute("ack", data.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_cosmos_receive_processing", "success")
                .add_event(euclid_event)
                .add_event(write_acknowledge_event)
                .set_data(data))
        }
    }
}
