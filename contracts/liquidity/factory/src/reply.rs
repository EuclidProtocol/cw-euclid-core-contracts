use crate::state::{TOKEN_TO_CW20, TOKEN_TO_ESCROW};
use cosmwasm_std::{from_json, DepsMut, Reply, Response, SubMsgResult};
use cw0::{parse_execute_response_data, parse_reply_instantiate_data};
use euclid::error::ContractError;
use euclid_ibc::ack::make_ack_fail;

pub const ESCROW_INSTANTIATE_REPLY_ID: u64 = 1;
pub const IBC_ACK_AND_TIMEOUT_REPLY_ID: u64 = 2;
pub const IBC_RECEIVE_REPLY_ID: u64 = 3;
pub const CW20_INSTANTIATE_REPLY_ID: u64 = 4;

pub fn on_escrow_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data: cw0::MsgInstantiateContractResponse =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let escrow_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let escrow_data: euclid::msgs::escrow::EscrowInstantiateResponse =
                from_json(instantiate_data.data.unwrap_or_default())?;

            TOKEN_TO_ESCROW.save(deps.storage, escrow_data.token.clone(), &escrow_address)?;
            Ok(Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("escrow", escrow_address)
                .add_attribute("token_id", escrow_data.token.to_string()))
        }
    }
}

pub fn on_cw20_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::PoolInstantiateFailed { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data: cw0::MsgInstantiateContractResponse =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let cw20_address = deps.api.addr_validate(&instantiate_data.contract_address)?;
            let cw20_data: euclid::msgs::escrow::Cw20InstantiateResponse =
                from_json(instantiate_data.data.unwrap_or_default())?;

            TOKEN_TO_CW20.save(deps.storage, cw20_data.token.clone(), &cw20_address)?;
            Ok(Response::new()
                .add_attribute("action", "reply_pool_instantiate")
                .add_attribute("cw20", cw20_address)
                .add_attribute("token_id", cw20_data.token.to_string()))
        }
    }
}

pub fn on_ibc_ack_and_timeout_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            Ok(Response::new().add_attribute("reply_err_on_ibc_ack_or_timeout_processing", err))
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
            Ok(Response::new()
                .add_attribute("action", "reply_sucess_on_ibc_ack_or_timeout_processing")
                .set_data(data))
        }
    }
}

pub fn on_ibc_receive_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Ok(Response::new()
            .add_attribute("reply_err_on_ibc_receive_processing", err.clone())
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
                .add_attribute("action", "reply_sucess_on_ibc_receive_processing")
                .set_data(data))
        }
    }
}
