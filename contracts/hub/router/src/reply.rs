use cosmwasm_std::{from_json, to_json_binary, DepsMut, Reply, Response, SubMsgResult};
use cw0::{parse_reply_execute_data, parse_reply_instantiate_data};
use euclid::{error::ContractError, msgs, pool::PoolCreationResponse};
use euclid_ibc::msg::AcknowledgementMsg;

use crate::state::VLPS;

pub const VLP_INSTANTIATE_REPLY_ID: u64 = 1u64;
pub const VLP_POOL_REGISTER_REPLY_ID: u64 = 2u64;

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
                (
                    liquidity.pair.token_1.get_token(),
                    liquidity.pair.token_2.get_token(),
                ),
                &vlp_address,
            )?;

            let pool_creation_response =
                from_json::<PoolCreationResponse>(instantiate_data.data.unwrap_or_default())?;
            let ack = AcknowledgementMsg::Ok(pool_creation_response);

            Ok(Response::new()
                .add_attribute("action", "reply_vlp_instantiate")
                .add_attribute("vlp", vlp_address)
                .set_data(to_json_binary(&ack)?))
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
