use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, Event, Reply, Response, SubMsg,
    SubMsgResult, WasmMsg,
};
use cw_utils::{
    parse_execute_response_data, parse_reply_execute_data, parse_reply_instantiate_data,
};
use euclid::{
    error::ContractError,
    events::simple_event,
    liquidity::{AddLiquidityResponse, RemoveLiquidityResponse},
    msgs::{self, router::ExecuteMsg, vlp::VlpRemoveLiquidityResponse},
    pool::{PoolCreationResponse, VlpSwapResponse},
    swap::SwapResponse,
};
use euclid_ibc::{
    ack::{make_ack_fail, AcknowledgementMsg},
    msg::HUB_IBC_EXECUTE_MSG_QUEUE,
};

use crate::{
    ibc::{self, receive::ibc_execute_add_liquidity},
    state::{FUNDS_INFO, PENDING_REMOVE_LIQUIDITY, STATE, SWAP_ID_TO_MSG, TOKEN_VLPS, VLPS},
};

pub const VLP_INSTANTIATE_REPLY_ID: u64 = 1;
pub const VLP_POOL_REGISTER_REPLY_ID: u64 = 2;
pub const ADD_LIQUIDITY_REPLY_ID: u64 = 3;
pub const REMOVE_LIQUIDITY_REPLY_ID: u64 = 4;
pub const SWAP_REPLY_ID: u64 = 5;

pub const VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID: u64 = 6;
pub const ESCROW_BALANCE_INSTANTIATE_REPLY_ID: u64 = 7;

pub const IBC_RECEIVE_REPLY_ID: u64 = 11;
pub const IBC_ACK_AND_TIMEOUT_REPLY_ID: u64 = 12;

pub const EVM_RECEIVE_REPLY_ID: u64 = 13;
pub const EVM_ACK_AND_TIMEOUT_REPLY_ID: u64 = 14;

pub const SOLANA_RECEIVE_REPLY_ID: u64 = 15;
pub const SOLANA_ACK_AND_TIMEOUT_REPLY_ID: u64 = 16;

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

            for token in &liquidity.pair.get_vec_token() {
                let key = TOKEN_VLPS.key(token.clone());
                let mut existing_vlps = key.may_load(deps.storage)?.unwrap_or_default();
                existing_vlps.push(vlp_address.clone());
                key.save(deps.storage, &existing_vlps)?;
            }

            VLPS.save(
                deps.storage,
                (liquidity.pair.token_1, liquidity.pair.token_2),
                &vlp_address,
            )?;
            let pool_creation_response = from_json::<PoolCreationResponse>(
                instantiate_data.data.clone().unwrap_or_default(),
            )?;
            let (funds, slippage_tolerance_bps) = FUNDS_INFO
                .load(deps.storage)
                .map_err(|_| ContractError::InsufficientFunds {})?;

            let response = ibc_execute_add_liquidity(
                deps,
                pool_creation_response.sender.clone(),
                funds,
                slippage_tolerance_bps,
                pool_creation_response.tx_id.clone(),
            )?;

            Ok(response
                .add_attribute("action", "reply_vlp_instantiate")
                .add_attribute("vlp", vlp_address))
        }
    }
}

pub fn on_pool_register_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
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
            let ack = AcknowledgementMsg::Ok(pool_creation_response.clone());

            let funds_info = FUNDS_INFO.may_load(deps.storage)?;

            let mut response = Response::new();
            if let Some((funds, slippage_tolerance_bps)) = funds_info {
                response = ibc_execute_add_liquidity(
                    deps,
                    pool_creation_response.sender,
                    funds,
                    slippage_tolerance_bps,
                    pool_creation_response.tx_id,
                )?;
            }

            Ok(response
                .add_attribute("action", "reply_pool_register")
                .add_attribute("vlp", vlp_address)
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_add_liquidity_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let liquidity_response: AddLiquidityResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let mut res = Response::new();
            let funds = FUNDS_INFO.may_load(deps.storage)?;
            match funds {
                Some(_) => {
                    let pool_response = PoolCreationResponse {
                        mint_lp_tokens: liquidity_response.mint_lp_tokens,
                        vlp_contract: liquidity_response.vlp_address.clone(),
                        tx_id: liquidity_response.tx_id.clone(),
                        sender: liquidity_response.sender.clone(),
                    };
                    FUNDS_INFO.remove(deps.storage);

                    let ack = AcknowledgementMsg::Ok(pool_response);
                    res = res.set_data(to_json_binary(&ack)?);
                }
                None => {
                    let ack = AcknowledgementMsg::Ok(liquidity_response.clone());
                    res = res.set_data(to_json_binary(&ack)?);
                }
            }

            Ok(res
                .add_attribute("action", "reply_add_liquidity")
                .add_attribute("liquidity", format!("{liquidity_response:?}")))
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
            let mut response = Response::new().add_attribute("action", "reply_remove_liquidity");

            let execute_data =
                parse_reply_execute_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let vlp_liquidity_response: VlpRemoveLiquidityResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let req_key = PENDING_REMOVE_LIQUIDITY.key((
                vlp_liquidity_response.sender.chain_uid.clone(),
                vlp_liquidity_response.sender.address.clone(),
                vlp_liquidity_response.tx_id.clone(),
            ));
            let remove_liquidity_tx = req_key.load(deps.storage)?;
            req_key.remove(deps.storage);

            for token in vlp_liquidity_response.liquidity_released.get_vec_token() {
                let token_escrow_release_msg =
                    euclid::msgs::router::ExecuteMsg::ReleaseEscrowInternal {
                        sender: remove_liquidity_tx.sender.clone(),
                        token: token.token.clone(),
                        amount: Some(token.amount),
                        cross_chain_addresses: remove_liquidity_tx.cross_chain_addresses.clone(),
                        timeout: None,
                        tx_id: vlp_liquidity_response.tx_id.clone(),
                    };

                let token_escrow_release_msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&token_escrow_release_msg)?,
                    funds: vec![],
                });
                response = response
                    .add_message(token_escrow_release_msg)
                    .add_attribute(
                        format!("token_removed_{}", token.token),
                        token.amount.to_string(),
                    );
            }

            let liquidity_response = RemoveLiquidityResponse {
                burn_lp_tokens: vlp_liquidity_response.burn_lp_tokens,
                vlp_address: vlp_liquidity_response.vlp_address,
                liquidity_removed: vlp_liquidity_response.liquidity_released,
            };

            let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

            Ok(response
                .add_attribute("liquidity", format!("{liquidity_response:?}"))
                .add_attribute("lp_burned", liquidity_response.burn_lp_tokens.to_string())
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

            let swap_req_key = SWAP_ID_TO_MSG.key((
                vlp_swap_response.sender.chain_uid,
                vlp_swap_response.sender.address,
                vlp_swap_response.tx_id.clone(),
            ));
            let swap_msg = swap_req_key.load(deps.storage)?;
            swap_req_key.remove(deps.storage);

            ensure!(
                vlp_swap_response.asset_out == swap_msg.asset_out,
                ContractError::new("Asset Out Mismatch")
            );

            ensure!(
                vlp_swap_response.amount_out >= swap_msg.min_amount_out,
                ContractError::SlippageExceeded {
                    amount: vlp_swap_response.amount_out,
                    min_amount_out: swap_msg.min_amount_out
                }
            );

            let swap_response = SwapResponse {
                amount_out: vlp_swap_response.amount_out,
                tx_id: vlp_swap_response.tx_id,
            };

            // Prepare burn msg
            let release_msg = ExecuteMsg::ReleaseEscrowInternal {
                sender: swap_msg.sender,
                token: swap_msg.asset_out.clone(),
                amount: Some(swap_response.amount_out),
                cross_chain_addresses: swap_msg.cross_chain_addresses,
                timeout: None,
                tx_id: swap_msg.tx_id.clone(),
            };
            let swap_response = SwapResponse {
                amount_out: swap_response.amount_out,
                tx_id: swap_msg.tx_id,
            };

            let ack = AcknowledgementMsg::Ok(swap_response.clone());

            Ok(Response::new()
                .add_submessage(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&release_msg)?,
                    funds: vec![],
                })))
                .add_attribute("action", "reply_swap")
                .add_attribute("swap", format!("{swap_response:?}"))
                .add_attribute("amount_out", swap_response.amount_out)
                .add_attribute("asset_out", swap_msg.asset_out.to_string())
                .add_attribute("asset_in", swap_msg.asset_in.token.to_string())
                .add_attribute("asset_type", swap_msg.asset_in.token_type.get_key())
                .add_attribute("amount_in", swap_msg.amount_in)
                .set_data(to_json_binary(&ack)?))
        }
    }
}

pub fn on_virtual_balance_instantiate_reply(
    deps: DepsMut,
    msg: Reply,
) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Generic { err }),
        SubMsgResult::Ok(..) => {
            let instantiate_data =
                parse_reply_instantiate_data(msg).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let mut state = STATE.load(deps.storage)?;
            state.virtual_balance_address =
                Some(deps.api.addr_validate(&instantiate_data.contract_address)?);
            STATE.save(deps.storage, &state)?;

            Ok(Response::new()
                .add_attribute("action", "reply_virtual_balance_instantiate")
                .add_attribute("virtual_balance_address", instantiate_data.contract_address))
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
    let chain_type = euclid::chain::ChainType::Native {};
    let original_msg = HUB_IBC_EXECUTE_MSG_QUEUE.load(deps.storage, msg.id)?;
    HUB_IBC_EXECUTE_MSG_QUEUE.remove(deps.storage, msg.id);
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let ack = make_ack_fail(err.clone())?;
            let response = ibc::ack_and_timeout::reusable_internal_ack_call(
                deps,
                env,
                original_msg,
                ack,
                chain_type,
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
                chain_type,
            )?;
            Ok(response.add_attribute("reply_on_ibc_receive_processing", "success"))
        }
    }
}

pub fn on_evm_receive_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let euclid_event = simple_event().add_attribute("action", "evm-relay");

            let write_acknowledge_event = Event::new("euclid-evm-write-acknowledgement")
                .add_attribute("ack", make_ack_fail(err.clone())?.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_evm_receive_processing", "error")
                .add_attribute("error", err.clone())
                .add_event(euclid_event)
                .add_event(write_acknowledge_event))
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

            let euclid_event = simple_event().add_attribute("action", "evm-write-acknowledgement");

            let write_acknowledge_event = Event::new("euclid-evm-write-acknowledgement")
                .add_attribute("ack", data.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_evm_receive_processing", "success")
                .add_event(euclid_event)
                .add_event(write_acknowledge_event)
                .set_data(data))
        }
    }
}

pub fn on_solana_receive_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let euclid_event = simple_event().add_attribute("action", "evm-relay");

            let write_acknowledge_event = Event::new("euclid-solana-write-acknowledgement")
                .add_attribute("ack", make_ack_fail(err.clone())?.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_solana_receive_processing", "error")
                .add_attribute("error", err.clone())
                .add_event(euclid_event)
                .add_event(write_acknowledge_event))
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

            let euclid_event =
                simple_event().add_attribute("action", "solana-write-acknowledgement");

            let write_acknowledge_event = Event::new("euclid-solana-write-acknowledgement")
                .add_attribute("ack", data.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_solana_receive_processing", "success")
                .add_event(euclid_event)
                .add_event(write_acknowledge_event)
                .set_data(data))
        }
    }
}
