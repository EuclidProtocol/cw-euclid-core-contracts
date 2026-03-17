use cosmwasm_std::{
    ensure, from_json, to_json_binary, DepsMut, Env, Event, Reply, Response, SubMsgResult,
};
use cw_utils::{parse_execute_response_data, parse_instantiate_response_data};
use euclid::{
    error::ContractError,
    events::{simple_event, EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT},
    liquidity::{
        AddLiquidityResponse, ConcentratedAddLiquidityResponse, ConcentratedCollectFeesResponse,
        ConcentratedCollectProtocolFeesResponse, ConcentratedRemoveLiquidityResponse,
        RemoveLiquidityResponse,
    },
    msgs::{
        self,
        vlp::base::{
            ConcentratedPoolCreationResponse, PoolCreationResponse,
            VlpConcentratedAddLiquidityResponse, VlpConcentratedCollectFeesResponse,
            VlpConcentratedCollectProtocolFeesResponse, VlpConcentratedRemoveLiquidityResponse,
            VlpRemoveLiquidityResponse, VlpSwapResponse,
        },
    },
    swap::SwapResponse,
};
use euclid_ibc::{
    ack::{make_ack_fail, AcknowledgementMsg},
    factory_ibc::FactoryCrossChainExecuteMsg,
    state::NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE,
};
use function_name::named;

use crate::{
    execute::token::execute_transfer_voucher,
    ibc::{
        self,
        receive::pool::{ibc_execute_add_concentrated_liquidity, ibc_execute_add_liquidity},
    },
    state::{
        pool_key_to_map_key, CONCENTRATED_FUNDS_INFO, CONCENTRATED_VLPS, FUNDS_INFO,
        PENDING_CONCENTRATED_COLLECT_FEES, PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES,
        PENDING_CONCENTRATED_REMOVE_LIQUIDITY, PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, TOKEN_VLPS,
        VIRTUAL_BALANCE_CONTRACT, VLPS,
    },
};

pub const VLP_INSTANTIATE_REPLY_ID: u64 = 1;
pub const VLP_POOL_REGISTER_REPLY_ID: u64 = 2;
pub const ADD_LIQUIDITY_REPLY_ID: u64 = 3;
pub const REMOVE_LIQUIDITY_REPLY_ID: u64 = 4;
pub const SWAP_REPLY_ID: u64 = 5;
pub const COLLECT_CONCENTRATED_REPLY_ID: u64 = 9;

pub const VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID: u64 = 6;
pub const ESCROW_BALANCE_INSTANTIATE_REPLY_ID: u64 = 7;

pub const CROSS_CHAIN_RECEIVE_REPLY_ID: u64 = 8;

pub fn on_vlp_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::InstantiateError { err }),
        SubMsgResult::Ok(result) => {
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let instantiate_data =
                parse_instantiate_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let vlp_address = instantiate_data.contract_address;
            let vlp_address = deps.api.addr_validate(&vlp_address)?;
            let data = instantiate_data.data.clone().unwrap_or_default();
            if let Ok(pool_creation_response) =
                from_json::<ConcentratedPoolCreationResponse>(data.clone())
            {
                for token in &pool_creation_response.pool_key.pair.get_vec_token() {
                    let key = TOKEN_VLPS.key(token.clone());
                    let mut existing_vlps = key.may_load(deps.storage)?.unwrap_or_default();
                    existing_vlps.push(vlp_address.clone());
                    key.save(deps.storage, &existing_vlps)?;
                }
                CONCENTRATED_VLPS.save(
                    deps.storage,
                    pool_key_to_map_key(&pool_creation_response.pool_key),
                    &vlp_address,
                )?;

                let funds_info = CONCENTRATED_FUNDS_INFO
                    .may_load(deps.storage)?
                    .ok_or(ContractError::InsufficientFunds {})?;
                let response = ibc_execute_add_concentrated_liquidity(
                    deps,
                    euclid_ibc::router_ibc::RouterCrossChainConcentratedAddLiquidityExecuteMsg {
                        sender: pool_creation_response.sender.clone(),
                        pair: funds_info.pair_with_denom,
                        pool_key: funds_info.pool_key,
                        lower_tick_index: funds_info.lower_tick_index,
                        upper_tick_index: funds_info.upper_tick_index,
                        position_id: funds_info.position_id,
                        slippage_tolerance_bps: funds_info.slippage_tolerance_bps,
                        tx_id: pool_creation_response.tx_id.clone(),
                    },
                )?;
                return Ok(response
                    .add_attribute("action", "reply_vlp_instantiate")
                    .add_attribute("pool_type", "concentrated")
                    .add_attribute("vlp", vlp_address));
            }

            let liquidity: msgs::vlp::base::GetLiquidityQueryResponse =
                deps.querier.query_wasm_smart(
                    vlp_address.to_string(),
                    &msgs::vlp::base::QueryMsg::Liquidity {},
                )?;

            for token in &liquidity.pair.get_vec_token() {
                let key = TOKEN_VLPS.key(token.clone());
                let mut existing_vlps = key.may_load(deps.storage)?.unwrap_or_default();
                existing_vlps.push(vlp_address.clone());
                key.save(deps.storage, &existing_vlps)?;
            }

            VLPS.save(deps.storage, liquidity.pair.get_tupple(), &vlp_address)?;
            let pool_creation_response = from_json::<PoolCreationResponse>(data)?;
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
                .add_attribute("pool_type", "classic")
                .add_attribute("vlp", vlp_address))
        }
    }
}

#[named]
pub fn on_pool_register_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let data = execute_data.data.unwrap_or_default();
            if let Ok(pool_creation_response) =
                from_json::<ConcentratedPoolCreationResponse>(data.clone())
            {
                let vlp_address = pool_creation_response.vlp_contract.clone();
                let ack = AcknowledgementMsg::Ok(pool_creation_response.clone());
                let mut response = Response::new();
                if let Some(funds_info) = CONCENTRATED_FUNDS_INFO.may_load(deps.storage)? {
                    response = ibc_execute_add_concentrated_liquidity(
                        deps,
                        euclid_ibc::router_ibc::RouterCrossChainConcentratedAddLiquidityExecuteMsg {
                            sender: pool_creation_response.sender,
                            pair: funds_info.pair_with_denom,
                            pool_key: funds_info.pool_key,
                            lower_tick_index: funds_info.lower_tick_index,
                            upper_tick_index: funds_info.upper_tick_index,
                            position_id: funds_info.position_id,
                            slippage_tolerance_bps: funds_info.slippage_tolerance_bps,
                            tx_id: pool_creation_response.tx_id,
                        },
                    )?;
                }
                return Ok(response
                    .add_attribute("action", "reply_pool_register")
                    .add_attribute("pool_type", "concentrated")
                    .add_attribute("vlp", vlp_address)
                    .set_data(to_json_binary(&ack)?));
            }

            let pool_creation_response: PoolCreationResponse = from_json(data)?;
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
                .add_attribute("pool_type", "classic")
                .add_attribute("vlp", vlp_address)
                .set_data(to_json_binary(&ack)?))
        }
    }
}

#[named]
pub fn on_add_liquidity_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let data = execute_data.data.unwrap_or_default();
            if let Ok(liquidity_response) =
                from_json::<VlpConcentratedAddLiquidityResponse>(data.clone())
            {
                let mut res = Response::new();
                if CONCENTRATED_FUNDS_INFO.may_load(deps.storage)?.is_some() {
                    CONCENTRATED_FUNDS_INFO.remove(deps.storage);
                }
                let ack = AcknowledgementMsg::Ok(ConcentratedAddLiquidityResponse {
                    pool_key: liquidity_response.pool_key.clone(),
                    position_id: liquidity_response.position_id,
                    liquidity_delta: liquidity_response.liquidity_delta,
                    mint_lp_tokens: liquidity_response.liquidity_delta,
                    vlp_address: liquidity_response.vlp_address.clone(),
                    tx_id: liquidity_response.tx_id.clone(),
                    sender: liquidity_response.sender.clone(),
                });
                res = res.set_data(to_json_binary(&ack)?);
                return Ok(res
                    .add_attribute("action", "reply_add_liquidity")
                    .add_attribute("pool_type", "concentrated")
                    .add_attribute("liquidity", format!("{liquidity_response:?}")));
            }

            let liquidity_response: AddLiquidityResponse = from_json(data)?;

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
                    let ack: AcknowledgementMsg<AddLiquidityResponse> =
                        AcknowledgementMsg::Ok(liquidity_response.clone());
                    res = res.set_data(to_json_binary(&ack)?);
                }
            }

            Ok(res
                .add_attribute("action", "reply_add_liquidity")
                .add_attribute("pool_type", "classic")
                .add_attribute("liquidity", format!("{liquidity_response:?}")))
        }
    }
}

#[named]
pub fn on_remove_liquidity_reply(
    deps: DepsMut,
    _env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let response = Response::new().add_attribute("action", "reply_remove_liquidity");

            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let data = execute_data.data.unwrap_or_default();
            if let Ok(vlp_liquidity_response) =
                from_json::<VlpConcentratedRemoveLiquidityResponse>(data.clone())
            {
                let req_key =
                    PENDING_CONCENTRATED_REMOVE_LIQUIDITY.key(vlp_liquidity_response.tx_id.clone());
                let _remove_liquidity_tx = req_key.load(deps.storage)?;
                req_key.remove(deps.storage);

                let liquidity_response = ConcentratedRemoveLiquidityResponse {
                    pool_key: vlp_liquidity_response.pool_key,
                    position_id: vlp_liquidity_response.position_id,
                    liquidity_removed: vlp_liquidity_response.liquidity_released,
                    liquidity_delta: vlp_liquidity_response.liquidity_delta,
                    liquidity_after: vlp_liquidity_response.liquidity_after,
                    burn_lp_tokens: vlp_liquidity_response.liquidity_delta,
                    vlp_address: vlp_liquidity_response.vlp_address,
                    tx_id: vlp_liquidity_response.tx_id,
                    sender: vlp_liquidity_response.sender,
                };

                let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

                return Ok(response
                    .add_attribute("pool_type", "concentrated")
                    .add_attribute("liquidity", format!("{liquidity_response:?}"))
                    .add_attribute("lp_burned", liquidity_response.burn_lp_tokens.to_string())
                    .set_data(to_json_binary(&ack)?));
            }

            let vlp_liquidity_response: VlpRemoveLiquidityResponse = from_json(data)?;

            let req_key = PENDING_REMOVE_LIQUIDITY.key(vlp_liquidity_response.tx_id.clone());
            let _remove_liquidity_tx = req_key.load(deps.storage)?;
            req_key.remove(deps.storage);

            let liquidity_response = RemoveLiquidityResponse {
                burn_lp_tokens: vlp_liquidity_response.burn_lp_tokens,
                vlp_address: vlp_liquidity_response.vlp_address,
                liquidity_removed: vlp_liquidity_response.liquidity_released,
            };

            let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

            Ok(response
                .add_attribute("pool_type", "classic")
                .add_attribute("liquidity", format!("{liquidity_response:?}"))
                .add_attribute("lp_burned", liquidity_response.burn_lp_tokens.to_string())
                .set_data(to_json_binary(&ack)?))
        }
    }
}

#[named]
pub fn on_collect_concentrated_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let response = Response::new().add_attribute("action", "reply_collect_concentrated");

            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let data = execute_data.data.unwrap_or_default();

            if let Ok(vlp_collect_response) =
                from_json::<VlpConcentratedCollectFeesResponse>(data.clone())
            {
                let req_key =
                    PENDING_CONCENTRATED_COLLECT_FEES.key(vlp_collect_response.tx_id.clone());
                let _req = req_key.load(deps.storage)?;
                req_key.remove(deps.storage);

                let collect_response = ConcentratedCollectFeesResponse {
                    pool_key: vlp_collect_response.pool_key,
                    position_id: vlp_collect_response.position_id,
                    amount_0: vlp_collect_response.amount_0,
                    amount_1: vlp_collect_response.amount_1,
                    vlp_address: vlp_collect_response.vlp_address,
                    tx_id: vlp_collect_response.tx_id,
                    sender: vlp_collect_response.sender,
                    recipient: vlp_collect_response.recipient,
                };
                let ack = AcknowledgementMsg::Ok(collect_response.clone());

                return Ok(response
                    .add_attribute("collect_type", "position_fees")
                    .add_attribute("amount_0", collect_response.amount_0)
                    .add_attribute("amount_1", collect_response.amount_1)
                    .set_data(to_json_binary(&ack)?));
            }

            if let Ok(vlp_collect_response) =
                from_json::<VlpConcentratedCollectProtocolFeesResponse>(data.clone())
            {
                let req_key = PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES
                    .key(vlp_collect_response.tx_id.clone());
                let _req = req_key.load(deps.storage)?;
                req_key.remove(deps.storage);

                let collect_response = ConcentratedCollectProtocolFeesResponse {
                    pool_key: vlp_collect_response.pool_key,
                    amount_0: vlp_collect_response.amount_0,
                    amount_1: vlp_collect_response.amount_1,
                    vlp_address: vlp_collect_response.vlp_address,
                    tx_id: vlp_collect_response.tx_id,
                    sender: vlp_collect_response.sender,
                    recipient: vlp_collect_response.recipient,
                };
                let ack = AcknowledgementMsg::Ok(collect_response.clone());

                return Ok(response
                    .add_attribute("collect_type", "protocol_fees")
                    .add_attribute("amount_0", collect_response.amount_0)
                    .add_attribute("amount_1", collect_response.amount_1)
                    .set_data(to_json_binary(&ack)?));
            }

            Err(ContractError::new(
                "invalid concentrated collect reply payload",
            ))
        }
    }
}

#[named]
pub fn on_swap_reply(deps: &mut DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let vlp_swap_response: VlpSwapResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let swap_req_key = PENDING_SWAPS.key(vlp_swap_response.tx_id.clone());
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

            let response = execute_transfer_voucher(
                deps,
                env,
                swap_msg.sender.clone(),
                swap_msg.asset_out.clone(),
                swap_response.amount_out,
                swap_msg.recipients.clone(),
            )?;

            let ack = AcknowledgementMsg::Ok(swap_response.clone());

            Ok(response
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

#[named]
pub fn on_virtual_balance_instantiate_reply(
    deps: DepsMut,
    msg: Reply,
) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(..) => {
            let msg_clone = msg.clone();
            let result = msg_clone.result.unwrap();
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();

            let instantiate_data =
                parse_instantiate_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;

            let verified_vcoin_address =
                deps.api.addr_validate(&instantiate_data.contract_address)?;
            VIRTUAL_BALANCE_CONTRACT.save(deps.storage, &verified_vcoin_address)?;

            Ok(Response::new()
                .add_attribute("action", "reply_virtual_balance_instantiate")
                .add_attribute("virtual_balance_address", instantiate_data.contract_address))
        }
    }
}

pub fn on_reply_native_ibc_wrapper_call(
    deps: DepsMut,
    env: Env,
    msg: Reply,
) -> Result<Response, ContractError> {
    let chain_type = euclid::chain::ChainType::Native {};
    let original_packet = NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.load(deps.storage, msg.id)?;
    NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.remove(deps.storage, msg.id);
    let original_msg: FactoryCrossChainExecuteMsg = from_json(original_packet.original_msg)?;
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let ack = make_ack_fail(err.clone())?;
            let response = ibc::ack_and_timeout::reusable_internal_ack_call(
                deps,
                env,
                original_packet.chain_uid,
                original_msg,
                ack,
                chain_type,
            )?;
            Ok(response
                .add_attribute("reply_on_native_ibc_wrapper_call_processing", "err")
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
                original_packet.chain_uid,
                original_msg,
                data,
                chain_type,
            )?;
            Ok(response.add_attribute("reply_on_native_ibc_wrapper_call_processing", "success"))
        }
    }
}

pub fn on_cross_chain_receive_reply(_deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => {
            let euclid_event = simple_event().add_attribute("action", "cross-chain-receive");

            let write_acknowledge_event = Event::new(EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT)
                .add_attribute("ack", make_ack_fail(err.clone())?.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_receive_processing", "error")
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
                simple_event().add_attribute("action", EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT);

            let write_acknowledge_event = Event::new(EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT)
                .add_attribute("ack", data.to_string());

            Ok(Response::new()
                .add_attribute("reply_on_receive_processing", "success")
                .add_event(euclid_event)
                .add_event(write_acknowledge_event)
                .set_data(data))
        }
    }
}
