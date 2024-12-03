use crate::{
    ibc,
    query::get_contract_code_hash,
    state::{PENDING_DEPOSIT_TOKEN, TOKEN_TO_ESCROW, VLP_TO_SNIP20},
};
use cosmwasm_std::{from_binary, DepsMut, Env, Reply, Response, SubMsgResponse, SubMsgResult};
use euclid::{chain::AnyContractInfo, error::ContractError};
use euclid_ibc::{ack::make_ack_fail, msg::CHAIN_IBC_EXECUTE_MSG_QUEUE};
use secret_utils::parse_execute_response_data;

pub const ESCROW_INSTANTIATE_REPLY_ID: u64 = 1;
pub const IBC_ACK_AND_TIMEOUT_REPLY_ID: u64 = 2;
pub const IBC_RECEIVE_REPLY_ID: u64 = 3;
pub const SNIP20_INSTANTIATE_REPLY_ID: u64 = 4;

pub fn on_escrow_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(res) => {
            // let instantiate_data: secret_utils::MsgInstantiateContractResponse =
            //     parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
            //         err: res.to_string(),
            //     })?;

            let escrow_address = deps
                .api
                .addr_validate(&parse_reply_address_from_event(res.clone()))?;
            let escrow_code_hash =
                get_contract_code_hash(deps.querier, escrow_address.clone().to_string())?;
            let escrow_data: euclid::msgs::escrow::EscrowInstantiateResponse =
                from_binary(&res.data.unwrap_or_default())?;
            let escrow_info = AnyContractInfo {
                addr: escrow_address.clone(),
                code_hash: escrow_code_hash.clone(),
            };

            TOKEN_TO_ESCROW.insert(deps.storage, &escrow_data.token.clone(), &escrow_info)?;

            let mut response = Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("escrow", escrow_address.clone())
                .add_attribute("token_id", escrow_data.token.to_string());

            let pending_deposit_token =
                PENDING_DEPOSIT_TOKEN.get(deps.storage, &escrow_data.token.clone());

            match pending_deposit_token {
                Some(token) => {
                    let deposit_msg = token.token_type.create_escrow_msg(
                        token.amount,
                        escrow_address,
                        escrow_code_hash,
                        None,
                        None,
                        None,
                        None,
                    )?;
                    response = response.add_message(deposit_msg);
                    PENDING_DEPOSIT_TOKEN.remove(deps.storage, &token.token)?;
                }
                None => {}
            }

            Ok(response)
        }
    }
}

pub fn on_snip20_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(res) => {
            // let instantiate_data: secret_utils::MsgInstantiateContractResponse =
            //     parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
            //         err: res.to_string(),
            //     })?;

            let snip20_address = deps
                .api
                .addr_validate(&parse_reply_address_from_event(res.clone()))?;
            let snip20_data: euclid::msgs::escrow::Snip20InstantiateResponse =
                from_binary(&res.data.unwrap_or_default())?;

            VLP_TO_SNIP20.insert(deps.storage, &snip20_data.vlp, &snip20_address)?;
            Ok(Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("cw20", snip20_address))
        }
    }
}

pub fn on_ibc_ack_and_timeout_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Ok(Response::new()
            .add_attribute("reply_on_ibc_ack_or_timeout_processing", "error")
            .add_attribute("error", err)),
        SubMsgResult::Ok(res) => {
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
    let original_msg = CHAIN_IBC_EXECUTE_MSG_QUEUE
        .get(deps.storage, &msg.id)
        .unwrap();
    CHAIN_IBC_EXECUTE_MSG_QUEUE.remove(deps.storage, &msg.id)?;
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

pub fn parse_reply_address_from_event(res: SubMsgResponse) -> String {
    let mut address = String::new();
    let mut found_address = false;

    for event in &res.events {
        if event.ty == "instantiate" {
            for attribute in &event.attributes {
                if attribute.key == "contract_address" || attribute.key == "_contract_addr" {
                    address.clone_from(&attribute.value);
                    found_address = true;
                    break;
                }
            }
        }
        if found_address {
            break;
        }
    }
    address
}
