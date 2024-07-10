use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, Reply, Response, SubMsgResult,
    WasmMsg,
};
use cw0::{parse_execute_response_data, parse_reply_execute_data, parse_reply_instantiate_data};
use euclid::{
    error::ContractError,
    liquidity::{AddLiquidityResponse, RemoveLiquidityResponse},
    msgs::{
        self,
        router::ExecuteMsg,
        vlp::{VlpRemoveLiquidityResponse, VlpSwapResponse},
    },
    pool::PoolCreationResponse,
    swap::SwapResponse,
};
use euclid_ibc::ack::{make_ack_fail, AcknowledgementMsg};

use crate::state::{PENDING_REMOVE_LIQUIDITY, STATE, SWAP_ID_TO_MSG, VLPS};

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

pub const IBC_RECEIVE_REPLY_ID: u64 = 11;
pub const IBC_ACK_AND_TIMEOUT_REPLY_ID: u64 = 12;

pub fn on_vlp_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::InstantiateError { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let vlp_address = instantiate_data.contract_address;

            let liquidity: msgs::vlp::GetLiquidityResponse = deps.querier.query_wasm_smart(
                vlp_address.clone(),
                &msgs::vlp::QueryMsg::Liquidity { height: None },
            )?;

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
            let liquidity_response: AddLiquidityResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

            Ok(Response::new()
                .add_attribute("action", "reply_add_liquidity")
                .add_attribute("liquidity", format!("{liquidity_response:?}"))
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_remove_liquidity_reply(
    deps: DepsMut,
    env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let vlp_liquidity_response: VlpRemoveLiquidityResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let req_key = (
                vlp_liquidity_response.sender.chain_uid.clone(),
                vlp_liquidity_response.sender.address.clone(),
                vlp_liquidity_response.tx_id.clone(),
            );
            let remove_liquidity_tx = PENDING_REMOVE_LIQUIDITY.load(deps.storage, req_key)?;

            let token_1_escrow_release_msg =
                euclid::msgs::router::ExecuteMsg::ReleaseEscrowInternal {
                    sender: vlp_liquidity_response.sender.clone(),
                    token: remove_liquidity_tx.pair.token_1.clone(),
                    amount: vlp_liquidity_response.token_1_liquidity,
                    cross_chain_addresses: remove_liquidity_tx.cross_chain_addresses.clone(),
                    timeout: None,
                    tx_id: vlp_liquidity_response.tx_id.clone(),
                };

            let token_1_escrow_release_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_json_binary(&token_1_escrow_release_msg)?,
                funds: vec![],
            });

            let token_2_escrow_release_msg =
                euclid::msgs::router::ExecuteMsg::ReleaseEscrowInternal {
                    sender: vlp_liquidity_response.sender,
                    token: remove_liquidity_tx.pair.token_1,
                    amount: vlp_liquidity_response.token_1_liquidity,
                    cross_chain_addresses: remove_liquidity_tx.cross_chain_addresses,
                    timeout: None,
                    tx_id: vlp_liquidity_response.tx_id,
                };
            let token_2_escrow_release_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_json_binary(&token_2_escrow_release_msg)?,
                funds: vec![],
            });

            let liquidity_response = RemoveLiquidityResponse {
                token_1_liquidity: vlp_liquidity_response.token_1_liquidity,
                token_2_liquidity: vlp_liquidity_response.token_2_liquidity,
                burn_lp_tokens: vlp_liquidity_response.burn_lp_tokens,
                reserve_1: vlp_liquidity_response.reserve_1,
                reserve_2: vlp_liquidity_response.reserve_2,
                vlp_address: vlp_liquidity_response.vlp_address,
            };

            let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

            Ok(Response::new()
                .add_attribute("action", "reply_remove_liquidity")
                .add_attribute("liquidity", format!("{liquidity_response:?}"))
                .add_message(token_1_escrow_release_msg)
                .add_message(token_2_escrow_release_msg)
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
            let vlp_swap_response: VlpSwapResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let req_key = (
                vlp_swap_response.sender.chain_uid,
                vlp_swap_response.sender.address,
                vlp_swap_response.tx_id.clone(),
            );
            let swap_msg = SWAP_ID_TO_MSG.load(deps.storage, req_key)?;

            ensure!(
                vlp_swap_response.asset_out == swap_msg.asset_out,
                ContractError::new("Asset Out Mismatch")
            );

            let swap_response = SwapResponse {
                amount_out: vlp_swap_response.amount_out,
                tx_id: vlp_swap_response.tx_id,
            };
            let ack = AcknowledgementMsg::Ok(swap_response.clone());

            // Prepare burn msg
            let release_msg = ExecuteMsg::ReleaseEscrowInternal {
                sender: swap_msg.sender,
                token: swap_msg.asset_out,
                amount: swap_response.amount_out,
                cross_chain_addresses: swap_msg.cross_chain_addresses,
                timeout: None,
                tx_id: swap_msg.tx_id,
            };

            Ok(Response::new()
                .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&release_msg)?,
                    funds: vec![],
                }))
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
