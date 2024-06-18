use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, Reply,
    Response, SubMsgResult, Uint128,
};
use cw0::{parse_reply_execute_data, parse_reply_instantiate_data};
use euclid::{
    error::ContractError,
    msgs,
    pool::{LiquidityResponse, PoolCreationResponse, RemoveLiquidityResponse},
    swap::SwapResponse,
    timeout::get_timeout,
};
use euclid_ibc::msg::{AcknowledgementMsg, HubIbcExecuteMsg};

use crate::state::{CHAIN_ID_TO_CHAIN, ESCROW_BALANCES, STATE, SWAP_ID_TO_CHAIN_ID, VLPS};

pub const VLP_INSTANTIATE_REPLY_ID: u64 = 1;
pub const VLP_POOL_REGISTER_REPLY_ID: u64 = 2;
pub const ADD_LIQUIDITY_REPLY_ID: u64 = 3;
pub const REMOVE_LIQUIDITY_REPLY_ID: u64 = 4;
pub const SWAP_REPLY_ID: u64 = 5;

pub const VCOIN_INSTANTIATE_REPLY_ID: u64 = 6;
pub const ESCROW_BALANCE_INSTANTIATE_REPLY_ID: u64 = 7;

pub const VCOIN_MINT_REPLY_ID: u64 = 8;
pub const VCOIN_BURN_REPLY_ID: u64 = 9;
pub const VCOIN_TRANSFER_REPLY_ID: u64 = 10;

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

pub fn on_swap_reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let swap_response: SwapResponse = from_json(execute_data.data.unwrap_or_default())?;

            let ack = AcknowledgementMsg::Ok(swap_response.clone());
            let chain_id = SWAP_ID_TO_CHAIN_ID.load(deps.storage, swap_response.swap_id.clone())?;

            let chain = CHAIN_ID_TO_CHAIN.load(deps.storage, chain_id)?;

            let token_out_escrow_key = (swap_response.asset_out.clone(), chain.factory_chain_id);

            let token_out_escrow_balance = ESCROW_BALANCES
                .may_load(deps.storage, token_out_escrow_key.clone())?
                .unwrap_or(Uint128::zero());

            ensure!(
                token_out_escrow_balance.ge(&swap_response.amount_out),
                ContractError::Generic {
                    err: "Insufficient Escrow Balance on out chain".to_string()
                }
            );

            let packet = IbcMsg::SendPacket {
                channel_id: chain.from_hub_channel,
                data: to_json_binary(&HubIbcExecuteMsg::ReleaseEscrow {
                    router: env.contract.address.to_string(),
                })?,
                timeout: IbcTimeout::with_timestamp(
                    env.block.time.plus_seconds(get_timeout(None)?),
                ),
            };

            Ok(Response::new()
                .add_message(CosmosMsg::Ibc(packet))
                .add_attribute("action", "reply_swap")
                .add_attribute("swap", format!("{swap_response:?}"))
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_vcoin_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let mut state = STATE.load(deps.storage)?;
            state.vcoin_address = Some(deps.api.addr_validate(&instantiate_data.contract_address)?);
            STATE.save(deps.storage, &state)?;

            Ok(Response::new()
                .add_attribute("action", "reply_vcoin_instantiate")
                .add_attribute("vcoin_address", instantiate_data.contract_address))
        }
    }
}

pub fn on_vcoin_mint_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => Ok(Response::new()
            .add_attribute("action", "reply_mint_vcoin")
            .add_attribute("mint_success", "true")),
    }
}

pub fn on_vcoin_burn_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => Ok(Response::new()
            .add_attribute("action", "reply_burn_vcoin")
            .add_attribute("burn_success", "true")),
    }
}

pub fn on_vcoin_transfer_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => Ok(Response::new()
            .add_attribute("action", "reply_transfer_vcoin")
            .add_attribute("transfer_success", "true")),
    }
}
