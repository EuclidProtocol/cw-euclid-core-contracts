use cosmwasm_std::{from_json, to_json_binary, DepsMut, Reply, Response, SubMsgResult};
use cw0::{parse_reply_execute_data, parse_reply_instantiate_data};
use euclid::{
    error::ContractError,
    msgs,
    pool::{LiquidityResponse, PoolCreationResponse, RemoveLiquidityResponse},
    swap::SwapResponse,
};
use euclid_ibc::msg::AcknowledgementMsg;

use crate::state::VLPS;

pub const VLP_INSTANTIATE_REPLY_ID: u64 = 1;
pub const VLP_POOL_REGISTER_REPLY_ID: u64 = 2;
pub const ADD_LIQUIDITY_REPLY_ID: u64 = 3;
pub const REMOVE_LIQUIDITY_REPLY_ID: u64 = 4;
pub const SWAP_REPLY_ID: u64 = 5;

pub const VCOIN_INSTANTIATE_REPLY_ID: u64 = 6;
pub const ESCROW_BALANCE_INSTANTIATE_REPLY_ID: u64 = 7;

pub fn on_vlp_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::InstantiateError { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let vlp_address = instantiate_data.contract_address;

            let liquidity: msgs::vlp::GetLiquidityResponse = deps
                .querier
                .query_wasm_smart(vlp_address.clone(), &msgs::vlp::QueryMsg::Liquidity {})?;

            VLPS.save(
                deps.storage,
                (liquidity.pair.token_1, liquidity.pair.token_2),
                &vlp_address,
            )?;

            let pool_creation_response =
                from_json::<PoolCreationResponse>(instantiate_data.data.unwrap_or_default());

            // This is probably IBC Message so send ok Ack as data
            if pool_creation_response.is_ok() {
                let ack = AcknowledgementMsg::Ok(pool_creation_response?);

                Ok(Response::new()
                    .add_attribute("action", "reply_vlp_instantiate")
                    .add_attribute("vlp", vlp_address)
                    .add_attribute("action", "reply_pool_register")
                    .set_data(to_json_binary(&ack)?))
            } else {
                Ok(Response::new()
                    .add_attribute("action", "reply_vlp_instantiate")
                    .add_attribute("vlp", vlp_address))
            }
        }
    }
}

pub fn on_pool_register_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let pool_creation_response: PoolCreationResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let vlp_address = pool_creation_response.vlp_contract.clone();

            let ack = AcknowledgementMsg::Ok(pool_creation_response);

            Ok(Response::new()
                .add_attribute("action", "reply_pool_register")
                .add_attribute("vlp", vlp_address)
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_add_liquidity_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let liquidity_response: LiquidityResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

            Ok(Response::new()
                .add_attribute("action", "reply_add_liquidity")
                .add_attribute("liquidity", format!("{liquidity_response:?}"))
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_remove_liquidity_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let liquidity_response: RemoveLiquidityResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

            Ok(Response::new()
                .add_attribute("action", "reply_remove_liquidity")
                .add_attribute("liquidity", format!("{liquidity_response:?}"))
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_swap_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let swap_response: SwapResponse = from_json(execute_data.data.unwrap_or_default())?;

            let ack = AcknowledgementMsg::Ok(swap_response.clone());

            Ok(Response::new()
                .add_attribute("action", "reply_swap")
                .add_attribute("swap", format!("{swap_response:?}"))
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_vcoin_instantiate_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let vcoin_address = instantiate_data.contract_address;

            Ok(Response::new()
                .add_attribute("action", "reply_vcoin_instantiate")
                .add_attribute("vcoin_address", vcoin_address))
        }
    }
}
