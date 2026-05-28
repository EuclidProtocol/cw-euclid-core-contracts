use cosmwasm_std::{
    ensure, from_json, to_json_binary, DepsMut, Env, Event, Reply, Response, SubMsg, SubMsgResult,
    WasmMsg,
};
use cw_utils::{parse_execute_response_data, parse_instantiate_response_data};
use euclid::{
    chain::ChainUid,
    cross_chain_user::CrossChainUser,
    error::ContractError,
    events::{simple_event, EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT},
    fee::BPS_50_PERCENT,
    liquidity::{
        AddLiquidityResponse, ConcentratedAddLiquidityResponse, ConcentratedCollectFeesResponse,
        ConcentratedCollectProtocolFeesResponse, ConcentratedRemoveLiquidityResponse,
        RemoveLiquidityResponse,
    },
    msgs::{
        self,
        virtual_balance::msg::{ExecuteApprove, ExecuteMsg as VirtualBalanceMsg},
        vlp::base::{
            ConcentratedPoolCreationResponse, PoolCreationResponse, VlpAddLiquidityMsg,
            VlpAddLiquidityResponse, VlpConcentratedAddLiquidityResponse,
            VlpConcentratedCollectFeesResponse, VlpConcentratedCollectProtocolFeesResponse,
            VlpConcentratedRemoveLiquidityResponse, VlpRemoveLiquidityResponse, VlpSwapResponse,
        },
    },
    normalize::normalize_token_to_voucher,
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
    query::query_token_metadata_by_denom,
    state::{
        CLP_POSITION_ID_VLP_MAP, CONCENTRATED_FUNDS_INFO, CONCENTRATED_VLPS, FUNDS_INFO,
        PENDING_CONCENTRATED_COLLECT_FEES, PENDING_CONCENTRATED_COLLECT_PROTOCOL_FEES,
        PENDING_CONCENTRATED_REMOVE_LIQUIDITY, PENDING_REMOVE_LIQUIDITY,
        PENDING_SINGLE_SIDED_LIQUIDITY, PENDING_SWAPS, TOKEN_VLPS, VIRTUAL_BALANCE_CONTRACT, VLPS,
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

pub const SINGLE_SIDED_SWAP_REPLY_ID: u64 = 10;
pub const SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID: u64 = 11;

pub fn on_vlp_instantiate_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result {
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
                    pool_creation_response.pool_key.to_map_key(),
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
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
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
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
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
                // Remove funds info if it exists
                CONCENTRATED_FUNDS_INFO.remove(deps.storage);
                let ack = AcknowledgementMsg::Ok(ConcentratedAddLiquidityResponse {
                    position_id: liquidity_response.position_id,
                    liquidity_delta: liquidity_response.liquidity_delta,
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

            let liquidity_response: VlpAddLiquidityResponse = from_json(data)?;

            let mut res = Response::new();
            // Remove funds info if it exists
            FUNDS_INFO.remove(deps.storage);

            let add_liquidity_response = AddLiquidityResponse {
                mint_lp_tokens: liquidity_response.mint_lp_tokens,
                vlp_address: liquidity_response.vlp_address.clone(),
                tx_id: liquidity_response.tx_id.clone(),
                sender: liquidity_response.sender.clone(),
            };

            let ack: AcknowledgementMsg<AddLiquidityResponse> =
                AcknowledgementMsg::Ok(add_liquidity_response.clone());
            res = res.set_data(to_json_binary(&ack)?);

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
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
            let response = Response::new().add_attribute("action", "reply_remove_liquidity");

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

                if vlp_liquidity_response.position_burned {
                    CLP_POSITION_ID_VLP_MAP
                        .remove(deps.storage, vlp_liquidity_response.position_id.u128());
                }

                let liquidity_response = ConcentratedRemoveLiquidityResponse {
                    pool_key: vlp_liquidity_response.pool_key,
                    position_id: vlp_liquidity_response.position_id,
                    liquidity_removed: vlp_liquidity_response.liquidity_released,
                    liquidity_delta: vlp_liquidity_response.liquidity_delta,
                    liquidity_after: vlp_liquidity_response.liquidity_after,
                    vlp_address: vlp_liquidity_response.vlp_address,
                    tx_id: vlp_liquidity_response.tx_id,
                    sender: vlp_liquidity_response.sender,
                    position_burned: vlp_liquidity_response.position_burned,
                };

                let ack = AcknowledgementMsg::Ok(liquidity_response.clone());

                return Ok(response
                    .add_attribute("pool_type", "concentrated")
                    .add_attribute("liquidity", format!("{liquidity_response:?}"))
                    .add_attribute("lp_burned", liquidity_response.liquidity_delta.to_string())
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
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
            let response = Response::new().add_attribute("action", "reply_collect_concentrated");

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
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
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

            // min_amount_out is already in voucher units (24 decimals)
            let normalized_min_amount_out = swap_msg.min_amount_out;

            ensure!(
                vlp_swap_response.amount_out >= normalized_min_amount_out,
                ContractError::SlippageExceeded {
                    amount: vlp_swap_response.amount_out,
                    min_amount_out: normalized_min_amount_out
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
    match msg.result {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
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

// Reply to the internal single-sided swap. The VLP has already deposited
// amount_out of asset_out into the user's virtual_balance via its terminal-hop
// Transfer. We just need to approve the same VLP to spend remaining asset_in
// and the swap output asset_out, then fire the add-liquidity submsg.
#[named]
pub fn on_single_sided_swap_reply(deps: DepsMut, msg: Reply) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();
            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let vlp_swap_response: VlpSwapResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let pending = PENDING_SINGLE_SIDED_LIQUIDITY
                .load(deps.storage, vlp_swap_response.tx_id.clone())?;

            // The swap output token is the "other" side of the target pair.
            let asset_out = pending.pair.get_other_token(pending.asset_in.token.clone());
            ensure!(
                vlp_swap_response.asset_out == asset_out,
                ContractError::new("asset_out mismatch on single-sided swap reply")
            );

            let virtual_balance_address = VIRTUAL_BALANCE_CONTRACT.load(deps.storage)?;

            // Compute the residual asset_in (in voucher units). This matches
            // the user's virtual_balance balance after the swap, because the
            // VLP consumed exactly normalized_swap_amount = normalize(swap_amount).
            // Re-querying metadata is safe: token metadata is set out-of-band
            // by admin and cannot change within a single transaction.
            let remaining_raw = pending.amount_in.checked_sub(pending.swap_amount)?;
            let remaining_normalized = if pending.asset_in.token_type.is_voucher() {
                remaining_raw
            } else {
                let metadata = query_token_metadata_by_denom(
                    deps.as_ref(),
                    &virtual_balance_address,
                    &pending.asset_in.token,
                    &pending.sender.chain_uid,
                    &pending.asset_in.token_type,
                )?;
                normalize_token_to_voucher(remaining_raw, metadata.token_type.get_decimals()?)?
            };

            // Target VLP (same as swap VLP in single-hop v1).
            let vlp_address = VLPS
                .may_load(deps.storage, pending.pair.get_tupple())?
                .ok_or(ContractError::PoolDoesNotExist {})?;

            let mut response = Response::new()
                .add_attribute("action", "single_sided_swap_reply")
                .add_attribute("tx_id", vlp_swap_response.tx_id.clone())
                .add_attribute("amount_out", vlp_swap_response.amount_out)
                .add_attribute("remaining_normalized", remaining_normalized);

            // Approve VLP to spend asset_in (remaining) on behalf of user.
            let approve_in_msg = VirtualBalanceMsg::Approve(ExecuteApprove {
                amount: remaining_normalized,
                token_id: pending.asset_in.token.to_string(),
                spender: CrossChainUser::new(ChainUid::vsl_chain_uid()?, vlp_address.to_string()),
                owner: pending.sender.clone(),
            });
            response = response.add_message(WasmMsg::Execute {
                contract_addr: virtual_balance_address.to_string(),
                msg: to_json_binary(&approve_in_msg)?,
                funds: vec![],
            });

            // Approve VLP to spend asset_out (swap output) on behalf of user.
            let approve_out_msg = VirtualBalanceMsg::Approve(ExecuteApprove {
                amount: vlp_swap_response.amount_out,
                token_id: asset_out.to_string(),
                spender: CrossChainUser::new(ChainUid::vsl_chain_uid()?, vlp_address.to_string()),
                owner: pending.sender.clone(),
            });
            response = response.add_message(WasmMsg::Execute {
                contract_addr: virtual_balance_address.to_string(),
                msg: to_json_binary(&approve_out_msg)?,
                funds: vec![],
            });

            // Pair tokens are canonically sorted, so match each side to its position.
            let (amount_1, amount_2) = if pending.pair.token_1 == pending.asset_in.token {
                (remaining_normalized, vlp_swap_response.amount_out)
            } else {
                (vlp_swap_response.amount_out, remaining_normalized)
            };
            let normalized_liquidity = pending.pair.get_pair_with_amount(amount_1, amount_2)?;

            let add_liquidity_msg = msgs::vlp::base::ExecuteMsg::AddLiquidity(VlpAddLiquidityMsg {
                sender: pending.sender.clone(),
                tx_id: vlp_swap_response.tx_id.clone(),
                liquidity: normalized_liquidity,
                slippage_tolerance_bps: BPS_50_PERCENT,
            });
            let add_liquidity_wasm = WasmMsg::Execute {
                contract_addr: vlp_address.to_string(),
                msg: to_json_binary(&add_liquidity_msg)?,
                funds: vec![],
            };

            Ok(response.add_submessage(SubMsg::reply_always(
                add_liquidity_wasm,
                SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID,
            )))
        }
    }
}

// Reply to the internal single-sided add-liquidity. Enforces min_lp_out;
// on success sets ack data to AcknowledgementMsg::Ok(AddLiquidityResponse).
// Slippage / submsg failures return Err so the outer cross-chain receive
// reply emits an error ack and all hub state rolls back.
#[named]
pub fn on_single_sided_add_liquidity_reply(
    deps: DepsMut,
    msg: Reply,
) -> Result<Response, ContractError> {
    match msg.result.clone() {
        SubMsgResult::Err(err) => Err(ContractError::Reply {
            action: function_name!().to_string(),
            err,
        }),
        SubMsgResult::Ok(result) => {
            #[allow(deprecated)]
            let data = result.data.unwrap_or_default();
            let execute_data =
                parse_execute_response_data(&data).map_err(|res| ContractError::Generic {
                    err: res.to_string(),
                })?;
            let liquidity_response: AddLiquidityResponse =
                from_json(execute_data.data.unwrap_or_default())?;

            let pending = PENDING_SINGLE_SIDED_LIQUIDITY
                .load(deps.storage, liquidity_response.tx_id.clone())?;

            ensure!(
                liquidity_response.mint_lp_tokens >= pending.min_lp_out,
                ContractError::SlippageExceeded {
                    amount: liquidity_response.mint_lp_tokens,
                    min_amount_out: pending.min_lp_out,
                }
            );

            PENDING_SINGLE_SIDED_LIQUIDITY.remove(deps.storage, liquidity_response.tx_id.clone());

            let ack = AcknowledgementMsg::Ok(liquidity_response.clone());
            Ok(Response::new()
                .add_attribute("action", "single_sided_add_liquidity_reply")
                .add_attribute("tx_id", liquidity_response.tx_id.clone())
                .add_attribute("mint_lp_tokens", liquidity_response.mint_lp_tokens)
                .add_attribute("vlp", liquidity_response.vlp_address)
                .set_data(to_json_binary(&ack)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        attr,
        testing::{message_info, mock_dependencies, mock_env, MockQuerier},
        Addr, Binary, Reply, SubMsgResponse, SubMsgResult, Uint256,
    };
    use euclid::{
        chain::ChainUid,
        cross_chain_user::CrossChainUser,
        msgs::vlp::base::{
            PoolCreationResponse, VlpAddLiquidityResponse, VlpRemoveLiquidityResponse,
            VlpSwapResponse,
        },
        token::{
            Pair, PairWithAmount, PairWithDenomAndAmount, Token, TokenType, TokenWithDenomAndAmount,
        },
    };
    use euclid_ibc::router_ibc::{
        RouterCrossChainRemoveLiquidityExecuteMsg, RouterCrossChainSwapExecuteMsg,
    };

    use crate::{
        contract::instantiate,
        reply::{
            on_add_liquidity_reply, on_cross_chain_receive_reply, on_pool_register_reply,
            on_remove_liquidity_reply, on_swap_reply, on_virtual_balance_instantiate_reply,
            ADD_LIQUIDITY_REPLY_ID, CROSS_CHAIN_RECEIVE_REPLY_ID, REMOVE_LIQUIDITY_REPLY_ID,
            SWAP_REPLY_ID, VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID, VLP_POOL_REGISTER_REPLY_ID,
        },
        state::{
            FUNDS_INFO, PENDING_REMOVE_LIQUIDITY, PENDING_SWAPS, TOKEN_DENOMS,
            VIRTUAL_BALANCE_CONTRACT, VLPS,
        },
    };
    use euclid::msgs::router::InstantiateMsg;

    type MockDeps = cosmwasm_std::OwnedDeps<
        cosmwasm_std::MemoryStorage,
        cosmwasm_std::testing::MockApi,
        MockQuerier,
    >;

    fn initialized() -> MockDeps {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            relayer_contract: Addr::unchecked("relayer"),
            release_fee_recipient: Addr::unchecked("release_fee_recipient"),
            default_fee_recipient: Addr::unchecked("default_fee_recipient"),
            constant_product_vlp_code_id: 1,
            stable_vlp_code_id: 3,
            virtual_balance_code_id: 2,
            concentrated_vlp_code_id: 4,
        };
        let sender = deps.api.addr_make("creator");
        let info = message_info(&sender, &[]);
        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        deps
    }

    // -----------------------------------------------------------------------
    // Protobuf encoding helpers
    //
    // cw_utils parses the Cosmos SDK proto responses manually (field 1 = string
    // for instantiate's contract_address, field 2 = bytes for optional data;
    // field 1 = bytes for execute data).  We reproduce the same wire format
    // without importing prost.
    // -----------------------------------------------------------------------

    /// Encode a protobuf varint into `out`.
    fn encode_varint(mut v: usize, out: &mut Vec<u8>) {
        loop {
            let mut byte = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if v == 0 {
                break;
            }
        }
    }

    /// Encode `field_number` with wire type 2 (length-delimited) followed by
    /// the given byte slice.
    fn encode_length_delimited_field(field_number: u8, data: &[u8], out: &mut Vec<u8>) {
        // tag = (field_number << 3) | 2
        out.push((field_number << 3) | 2u8);
        encode_varint(data.len(), out);
        out.extend_from_slice(data);
    }

    /// Build the raw bytes that `parse_instantiate_response_data` expects:
    ///   field 1: string  = contract_address
    ///   field 2: bytes   = inner_data  (optional; skipped when empty)
    fn encode_instantiate_response(contract_address: &str, inner_data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        encode_length_delimited_field(1, contract_address.as_bytes(), &mut out);
        if !inner_data.is_empty() {
            encode_length_delimited_field(2, inner_data, &mut out);
        }
        out
    }

    /// Build the raw bytes that `parse_execute_response_data` expects:
    ///   field 1: bytes = inner_data  (optional; skipped when empty)
    fn encode_execute_response(inner_data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        if !inner_data.is_empty() {
            encode_length_delimited_field(1, inner_data, &mut out);
        }
        out
    }

    /// Wrap pre-encoded protobuf bytes into a `Reply::Ok`.
    fn ok_reply(id: u64, proto_data: Vec<u8>) -> Reply {
        Reply {
            id,
            payload: Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![],
                #[allow(deprecated)]
                data: Some(Binary::new(proto_data)),
                msg_responses: vec![],
            }),
        }
    }

    /// Build a `Reply::Err`.
    fn err_reply(id: u64, err: &str) -> Reply {
        Reply {
            id,
            payload: Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Err(err.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // on_virtual_balance_instantiate_reply
    // -----------------------------------------------------------------------

    #[test]
    fn test_virtual_balance_instantiate_reply_ok_saves_address() {
        let mut deps = initialized();
        let vb_addr = deps.api.addr_make("virtual_balance");

        let proto_bytes = encode_instantiate_response(vb_addr.as_str(), &[]);
        let reply = ok_reply(VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID, proto_bytes);

        let res = on_virtual_balance_instantiate_reply(deps.as_mut(), reply).unwrap();

        // Attribute check
        assert_eq!(
            res.attributes[0],
            attr("action", "reply_virtual_balance_instantiate")
        );
        assert_eq!(
            res.attributes[1],
            attr("virtual_balance_address", vb_addr.as_str())
        );

        // State check
        let stored = VIRTUAL_BALANCE_CONTRACT
            .load(deps.as_ref().storage)
            .unwrap();
        assert_eq!(stored, vb_addr);
    }

    #[test]
    fn test_virtual_balance_instantiate_reply_err_returns_error() {
        let mut deps = initialized();
        let reply = err_reply(VIRTUAL_BALANCE_INSTANTIATE_REPLY_ID, "init failed");

        let err = on_virtual_balance_instantiate_reply(deps.as_mut(), reply).unwrap_err();
        assert!(matches!(err, euclid::error::ContractError::Reply { .. }));
    }

    // -----------------------------------------------------------------------
    // on_cross_chain_receive_reply
    // -----------------------------------------------------------------------

    #[test]
    fn test_cross_chain_receive_reply_ok_emits_events() {
        let mut deps = initialized();
        // Provide some arbitrary ack data wrapped in execute-response encoding
        let ack_bytes = b"ack_payload";
        let proto_bytes = encode_execute_response(ack_bytes);
        let reply = ok_reply(CROSS_CHAIN_RECEIVE_REPLY_ID, proto_bytes);

        let res = on_cross_chain_receive_reply(deps.as_mut(), reply).unwrap();

        // Top-level attribute
        assert_eq!(
            res.attributes[0],
            attr("reply_on_receive_processing", "success")
        );
        // Two events emitted
        assert_eq!(res.events.len(), 2);
        // The write-acknowledgement event contains the ack data
        let write_ack_event = &res.events[1];
        assert_eq!(
            write_ack_event.ty,
            euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT
        );
    }

    #[test]
    fn test_cross_chain_receive_reply_err_emits_error_events() {
        let mut deps = initialized();
        let reply = err_reply(CROSS_CHAIN_RECEIVE_REPLY_ID, "receive error");

        let res = on_cross_chain_receive_reply(deps.as_mut(), reply).unwrap();

        assert_eq!(
            res.attributes[0],
            attr("reply_on_receive_processing", "error")
        );
        assert_eq!(res.attributes[1], attr("error", "receive error"));
        // Two events: euclid event + write-acknowledgement event
        assert_eq!(res.events.len(), 2);
        assert_eq!(
            res.events[1].ty,
            euclid::events::EUCLID_WRITE_ACKNOWLEDGEMENT_EVENT
        );
    }

    // -----------------------------------------------------------------------
    // on_add_liquidity_reply — plain add-liquidity path (no FUNDS_INFO)
    // -----------------------------------------------------------------------

    fn make_add_liquidity_response(pair_with_amount: PairWithAmount) -> VlpAddLiquidityResponse {
        VlpAddLiquidityResponse {
            liquidity_added: pair_with_amount,
            mint_lp_tokens: Uint256::from(500u128),
            vlp_address: "vlp_contract".to_string(),
            tx_id: "tx-add-liq-1".to_string(),
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "user1".to_string(),
            ),
        }
    }

    #[test]
    fn test_add_liquidity_reply_no_funds_info_returns_add_liq_ack() {
        let mut deps = initialized();

        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();

        let liq_response = make_add_liquidity_response(
            pair.get_pair_with_amount(Uint256::from(100u128), Uint256::from(200u128))
                .unwrap(),
        );
        let inner_json = cosmwasm_std::to_json_binary(&liq_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(ADD_LIQUIDITY_REPLY_ID, proto_bytes);

        let res = on_add_liquidity_reply(deps.as_mut(), reply).unwrap();

        // Action attribute
        assert_eq!(res.attributes[0], attr("action", "reply_add_liquidity"));

        // Data should be set (ack for AddLiquidityResponse)
        assert!(res.data.is_some());

        // FUNDS_INFO should NOT be present (was not set)
        assert!(FUNDS_INFO
            .may_load(deps.as_ref().storage)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_add_liquidity_reply_with_funds_info_builds_add_liquidity_ack_and_clears_funds_info() {
        let mut deps = initialized();

        // Seed FUNDS_INFO (simulates pool-creation path)
        let token1 = Token::create("aaa".to_string()).unwrap();
        let token2 = Token::create("bbb".to_string()).unwrap();
        let pair_with_denom = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token1.clone(),
                amount: Uint256::from(100u128),
                token_type: TokenType::Voucher {},
            },
            token_2: TokenWithDenomAndAmount {
                token: token2.clone(),
                amount: Uint256::from(200u128),
                token_type: TokenType::Voucher {},
            },
        };
        FUNDS_INFO
            .save(deps.as_mut().storage, &(pair_with_denom.clone(), 50u64))
            .unwrap();

        let liq_response =
            make_add_liquidity_response(pair_with_denom.get_pair_with_amount().unwrap());
        let inner_json = cosmwasm_std::to_json_binary(&liq_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(ADD_LIQUIDITY_REPLY_ID, proto_bytes);

        let res = on_add_liquidity_reply(deps.as_mut(), reply).unwrap();

        assert_eq!(res.attributes[0], attr("action", "reply_add_liquidity"));
        // Data should be set (pool-creation ack wrapping PoolCreationResponse)
        assert!(res.data.is_some());
        // FUNDS_INFO must have been cleared
        assert!(FUNDS_INFO
            .may_load(deps.as_ref().storage)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_add_liquidity_reply_err_returns_error() {
        let mut deps = initialized();
        let reply = err_reply(ADD_LIQUIDITY_REPLY_ID, "add liq failed");

        let err = on_add_liquidity_reply(deps.as_mut(), reply).unwrap_err();
        assert!(matches!(err, euclid::error::ContractError::Reply { .. }));
    }

    // -----------------------------------------------------------------------
    // on_remove_liquidity_reply
    // -----------------------------------------------------------------------

    fn make_vlp_remove_liquidity_response(tx_id: &str) -> VlpRemoveLiquidityResponse {
        use euclid::token::{PairWithAmount, TokenWithAmount};
        VlpRemoveLiquidityResponse {
            liquidity_released: PairWithAmount::new(
                TokenWithAmount {
                    token: Token::create("aaa".to_string()).unwrap(),
                    amount: Uint256::from(100u128),
                },
                TokenWithAmount {
                    token: Token::create("bbb".to_string()).unwrap(),
                    amount: Uint256::from(200u128),
                },
            )
            .unwrap(),
            burn_lp_tokens: Uint256::from(50u128),
            tx_id: tx_id.to_string(),
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "user1".to_string(),
            ),
            vlp_address: "vlp_contract".to_string(),
        }
    }

    fn seed_pending_remove_liquidity(deps: &mut MockDeps, tx_id: &str) {
        let pair = Pair::new(
            Token::create("aaa".to_string()).unwrap(),
            Token::create("bbb".to_string()).unwrap(),
        )
        .unwrap();
        let msg = RouterCrossChainRemoveLiquidityExecuteMsg {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "user1".to_string(),
            ),
            lp_allocation: Uint256::from(50u128),
            pair,
            recipient: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "user1".to_string(),
            ),
            tx_id: tx_id.to_string(),
        };
        PENDING_REMOVE_LIQUIDITY
            .save(deps.as_mut().storage, tx_id.to_string(), &msg)
            .unwrap();
    }

    #[test]
    fn test_remove_liquidity_reply_ok_removes_pending_and_returns_ack() {
        let mut deps = initialized();
        let tx_id = "tx-remove-liq-1";
        seed_pending_remove_liquidity(&mut deps, tx_id);

        let vlp_response = make_vlp_remove_liquidity_response(tx_id);
        let inner_json = cosmwasm_std::to_json_binary(&vlp_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(REMOVE_LIQUIDITY_REPLY_ID, proto_bytes);

        let res = on_remove_liquidity_reply(deps.as_mut(), mock_env(), reply).unwrap();

        // Attribute check
        assert_eq!(res.attributes[0], attr("action", "reply_remove_liquidity"));
        assert_eq!(
            res.attributes[3],
            attr("lp_burned", vlp_response.burn_lp_tokens.to_string())
        );

        // Data/ack is set
        assert!(res.data.is_some());

        // Pending entry removed from state
        assert!(PENDING_REMOVE_LIQUIDITY
            .may_load(deps.as_ref().storage, tx_id.to_string())
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_remove_liquidity_reply_ok_missing_pending_returns_error() {
        let mut deps = initialized();
        // No PENDING_REMOVE_LIQUIDITY entry seeded

        let vlp_response = make_vlp_remove_liquidity_response("tx-missing");
        let inner_json = cosmwasm_std::to_json_binary(&vlp_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(REMOVE_LIQUIDITY_REPLY_ID, proto_bytes);

        assert!(on_remove_liquidity_reply(deps.as_mut(), mock_env(), reply).is_err());
    }

    #[test]
    fn test_remove_liquidity_reply_err_returns_error() {
        let mut deps = initialized();
        let reply = err_reply(REMOVE_LIQUIDITY_REPLY_ID, "remove liq failed");

        let err = on_remove_liquidity_reply(deps.as_mut(), mock_env(), reply).unwrap_err();
        assert!(matches!(err, euclid::error::ContractError::Reply { .. }));
    }

    // -----------------------------------------------------------------------
    // on_swap_reply
    // -----------------------------------------------------------------------

    fn seed_pending_swap(deps: &mut MockDeps, tx_id: &str, asset_out: Token, min_out: Uint256) {
        use euclid::token::TokenWithDenom;
        let msg = RouterCrossChainSwapExecuteMsg {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "swapper".to_string(),
            ),
            asset_in: TokenWithDenom {
                token: Token::create("aaa".to_string()).unwrap(),
                token_type: TokenType::Native {
                    denom: "uaaa".to_string(),
                    decimals: None,
                },
            },
            amount_in: Uint256::from(1000u128),
            asset_out: asset_out.clone(),
            min_amount_out: min_out,
            swaps: vec![],
            recipients: vec![],
            partner_fee_amount: Uint256::zero(),
            partner_fee_recipient: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "fee_recipient".to_string(),
            ),
            tx_id: tx_id.to_string(),
        };
        PENDING_SWAPS
            .save(deps.as_mut().storage, tx_id.to_string(), &msg)
            .unwrap();
    }

    fn seed_for_swap_reply(deps: &mut MockDeps, tx_id: &str, asset_out: Token, min_out: Uint256) {
        VIRTUAL_BALANCE_CONTRACT
            .save(deps.as_mut().storage, &Addr::unchecked("virtual_balance"))
            .unwrap();
        TOKEN_DENOMS
            .save(deps.as_mut().storage, asset_out.clone(), &vec![])
            .unwrap();
        seed_pending_swap(deps, tx_id, asset_out, min_out);
    }

    #[test]
    fn test_swap_reply_ok_happy_path() {
        let mut deps = initialized();
        let tx_id = "tx-swap-1";
        let asset_out = Token::create("bbb".to_string()).unwrap();
        let amount_out = Uint256::from(800u128);
        seed_for_swap_reply(&mut deps, tx_id, asset_out.clone(), Uint256::from(700u128));

        let vlp_swap_response = VlpSwapResponse {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "swapper".to_string(),
            ),
            tx_id: tx_id.to_string(),
            asset_out: asset_out.clone(),
            amount_out,
        };
        let inner_json = cosmwasm_std::to_json_binary(&vlp_swap_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(SWAP_REPLY_ID, proto_bytes);

        let res = on_swap_reply(&mut deps.as_mut(), mock_env(), reply).unwrap();

        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "action")
                .unwrap()
                .value,
            "reply_swap"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "amount_out")
                .unwrap()
                .value,
            "800"
        );
        assert_eq!(
            res.attributes
                .iter()
                .find(|a| a.key == "asset_out")
                .unwrap()
                .value,
            "bbb"
        );
        // Data/ack is set
        assert!(res.data.is_some());
        // PENDING_SWAPS entry is removed
        assert!(PENDING_SWAPS
            .may_load(deps.as_ref().storage, tx_id.to_string())
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_swap_reply_ok_asset_out_mismatch_returns_error() {
        let mut deps = initialized();
        let tx_id = "tx-swap-mismatch";
        let registered_asset_out = Token::create("bbb".to_string()).unwrap();
        seed_for_swap_reply(
            &mut deps,
            tx_id,
            registered_asset_out.clone(),
            Uint256::from(500u128),
        );

        // VLP returns a different asset_out
        let wrong_asset_out = Token::create("ccc".to_string()).unwrap();
        let vlp_swap_response = VlpSwapResponse {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "swapper".to_string(),
            ),
            tx_id: tx_id.to_string(),
            asset_out: wrong_asset_out,
            amount_out: Uint256::from(800u128),
        };
        let inner_json = cosmwasm_std::to_json_binary(&vlp_swap_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(SWAP_REPLY_ID, proto_bytes);

        let err = on_swap_reply(&mut deps.as_mut(), mock_env(), reply).unwrap_err();
        assert_eq!(err, euclid::error::ContractError::new("Asset Out Mismatch"));
    }

    #[test]
    fn test_swap_reply_ok_slippage_exceeded_returns_error() {
        let mut deps = initialized();
        let tx_id = "tx-swap-slippage";
        let asset_out = Token::create("bbb".to_string()).unwrap();
        // min_amount_out = 1000; amount_out will be 500 → slippage exceeded
        seed_for_swap_reply(&mut deps, tx_id, asset_out.clone(), Uint256::from(1000u128));

        let vlp_swap_response = VlpSwapResponse {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "swapper".to_string(),
            ),
            tx_id: tx_id.to_string(),
            asset_out: asset_out.clone(),
            amount_out: Uint256::from(500u128),
        };
        let inner_json = cosmwasm_std::to_json_binary(&vlp_swap_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(SWAP_REPLY_ID, proto_bytes);

        let err = on_swap_reply(&mut deps.as_mut(), mock_env(), reply).unwrap_err();
        assert!(matches!(
            err,
            euclid::error::ContractError::SlippageExceeded { .. }
        ));
    }

    #[test]
    fn test_swap_reply_err_returns_error() {
        let mut deps = initialized();
        let reply = err_reply(SWAP_REPLY_ID, "swap failed");

        let err = on_swap_reply(&mut deps.as_mut(), mock_env(), reply).unwrap_err();
        assert!(matches!(err, euclid::error::ContractError::Reply { .. }));
    }

    // Synthesizes the Q1(b) ack-direction failure mode: an inbound swap reply
    // references a `tx_id` that does not match any `PENDING_SWAPS` entry (the
    // state a reorg-replayed source would produce if its replayed `tx_id`
    // diverged from the one the destination already ack'd). The handler must
    // fail predictably rather than silently mutating state.
    #[test]
    fn test_swap_reply_ok_missing_pending_returns_error() {
        let mut deps = initialized();
        let asset_out = Token::create("bbb".to_string()).unwrap();
        // Seed for a known tx_id, but reply will reference a different one.
        seed_for_swap_reply(
            &mut deps,
            "tx-seeded",
            asset_out.clone(),
            Uint256::from(500u128),
        );

        let vlp_swap_response = VlpSwapResponse {
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "swapper".to_string(),
            ),
            tx_id: "tx-unknown".to_string(),
            asset_out,
            amount_out: Uint256::from(800u128),
        };
        let inner_json = cosmwasm_std::to_json_binary(&vlp_swap_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(SWAP_REPLY_ID, proto_bytes);

        let err = on_swap_reply(&mut deps.as_mut(), mock_env(), reply).unwrap_err();
        // Defined failure: load on a missing key surfaces as a contract error.
        // Importantly, NO panic, NO silent partial state change.
        assert!(
            format!("{err}").to_lowercase().contains("not found")
                || matches!(err, euclid::error::ContractError::Std(_)),
            "unexpected error variant: {err:?}"
        );

        // Seeded entry under the original tx_id is untouched.
        assert!(PENDING_SWAPS
            .may_load(deps.as_ref().storage, "tx-seeded".to_string())
            .unwrap()
            .is_some());
    }

    // -----------------------------------------------------------------------
    // on_pool_register_reply
    // -----------------------------------------------------------------------

    fn make_pool_creation_response(vlp_address: &str, tx_id: &str) -> PoolCreationResponse {
        PoolCreationResponse {
            vlp_contract: vlp_address.to_string(),
            tx_id: tx_id.to_string(),
            mint_lp_tokens: Uint256::from(1000u128),
            sender: CrossChainUser::new(
                ChainUid::create("chain1".to_string()).unwrap(),
                "user1".to_string(),
            ),
        }
    }

    #[test]
    fn test_pool_register_reply_ok_without_funds_info_returns_ack() {
        let mut deps = initialized();

        let pool_response = make_pool_creation_response("vlp_addr", "tx-pool-1");
        let inner_json = cosmwasm_std::to_json_binary(&pool_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(VLP_POOL_REGISTER_REPLY_ID, proto_bytes);

        // No FUNDS_INFO set — ibc_execute_add_liquidity should not be called
        let res = on_pool_register_reply(deps.as_mut(), reply).unwrap();

        assert_eq!(res.attributes[0], attr("action", "reply_pool_register"));
        assert_eq!(res.attributes[2], attr("vlp", "vlp_addr"));
        // Ack data is set
        assert!(res.data.is_some());
        // No submessages — ibc_execute_add_liquidity not triggered
        assert!(res.messages.is_empty());
    }

    #[test]
    fn test_pool_register_reply_ok_with_funds_info_calls_add_liquidity() {
        let mut deps = initialized();

        // Pre-seed state required by ibc_execute_add_liquidity
        let token1 = Token::create("aaa".to_string()).unwrap();
        let token2 = Token::create("bbb".to_string()).unwrap();
        let pair = Pair::new(token1.clone(), token2.clone()).unwrap();

        // Seed VLP address in VLPS so ibc_execute_add_liquidity can load it
        VLPS.save(
            deps.as_mut().storage,
            pair.get_tupple(),
            &Addr::unchecked("vlp_addr"),
        )
        .unwrap();
        VIRTUAL_BALANCE_CONTRACT
            .save(deps.as_mut().storage, &Addr::unchecked("virtual_balance"))
            .unwrap();

        let pair_with_denom = PairWithDenomAndAmount {
            token_1: TokenWithDenomAndAmount {
                token: token1.clone(),
                amount: Uint256::from(100u128),
                token_type: TokenType::Voucher {},
            },
            token_2: TokenWithDenomAndAmount {
                token: token2.clone(),
                amount: Uint256::from(200u128),
                token_type: TokenType::Voucher {},
            },
        };
        FUNDS_INFO
            .save(deps.as_mut().storage, &(pair_with_denom, 50u64))
            .unwrap();

        let pool_response = make_pool_creation_response("vlp_addr", "tx-pool-2");
        let inner_json = cosmwasm_std::to_json_binary(&pool_response).unwrap();
        let proto_bytes = encode_execute_response(&inner_json);
        let reply = ok_reply(VLP_POOL_REGISTER_REPLY_ID, proto_bytes);

        let res = on_pool_register_reply(deps.as_mut(), reply).unwrap();

        assert_eq!(res.attributes[0], attr("action", "reply_pool_register"));
        // With FUNDS_INFO set, ibc_execute_add_liquidity was called and messages were added
        assert!(!res.messages.is_empty());
    }

    #[test]
    fn test_pool_register_reply_err_returns_error() {
        let mut deps = initialized();
        let reply = err_reply(VLP_POOL_REGISTER_REPLY_ID, "pool register failed");

        let err = on_pool_register_reply(deps.as_mut(), reply).unwrap_err();
        assert!(matches!(err, euclid::error::ContractError::Reply { .. }));
    }

    // -----------------------------------------------------------------------
    // on_vlp_instantiate_reply — error path only
    // (the Ok path requires a live wasm querier for the Liquidity query,
    //  which is outside what mock_dependencies supports without a full
    //  multi-test setup)
    // -----------------------------------------------------------------------

    #[test]
    fn test_vlp_instantiate_reply_err_returns_instantiate_error() {
        let mut deps = initialized();
        let reply = err_reply(
            crate::reply::VLP_INSTANTIATE_REPLY_ID,
            "vlp instantiation failed",
        );

        let err = crate::reply::on_vlp_instantiate_reply(deps.as_mut(), reply).unwrap_err();
        assert!(matches!(
            err,
            euclid::error::ContractError::InstantiateError { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // single_sided reply handlers
    // -----------------------------------------------------------------------

    mod single_sided {
        use super::*;
        use cosmwasm_std::{
            from_json, to_json_binary, ContractResult, CosmosMsg, SystemResult, WasmMsg, WasmQuery,
        };
        use euclid::{
            liquidity::AddLiquidityResponse,
            msgs::virtual_balance::msg::{
                ExecuteMsg as VirtualBalanceMsg, GetTokenMetadataByDenomResponse,
                QueryMsg as VirtualBalanceQueryMsg,
            },
            swap::NextSwapPair,
            token::{TokenMetadata, TokenWithDenom},
        };
        use euclid_ibc::router_ibc::RouterCrossChainSingleSidedAddLiquidityMsg;

        use crate::{
            reply::{
                on_single_sided_add_liquidity_reply, on_single_sided_swap_reply,
                SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID, SINGLE_SIDED_SWAP_REPLY_ID,
            },
            state::PENDING_SINGLE_SIDED_LIQUIDITY,
        };

        fn token_aaa() -> Token {
            Token::create("aaa".to_string()).unwrap()
        }
        fn token_bbb() -> Token {
            Token::create("bbb".to_string()).unwrap()
        }

        fn seed_vlp(deps: &mut MockDeps) {
            let pair = Pair::new(token_aaa(), token_bbb()).unwrap();
            VLPS.save(
                deps.as_mut().storage,
                pair.get_tupple(),
                &Addr::unchecked("vlp_contract"),
            )
            .unwrap();
        }

        fn seed_virtual_balance(deps: &mut MockDeps) {
            VIRTUAL_BALANCE_CONTRACT
                .save(deps.as_mut().storage, &Addr::unchecked("virtual_balance"))
                .unwrap();
        }

        fn install_metadata_querier(deps: &mut MockDeps) {
            deps.querier.update_wasm(|q| match q {
                WasmQuery::Smart { msg, .. } => {
                    let parsed: VirtualBalanceQueryMsg = from_json(msg).unwrap();
                    match parsed {
                        VirtualBalanceQueryMsg::GetTokenMetadataByDenom {
                            token_id,
                            chain_uid,
                            token_type,
                        } => {
                            let token_type_with_decimals = match token_type {
                                TokenType::Native { denom, .. } => TokenType::Native {
                                    denom,
                                    decimals: Some(24),
                                },
                                other => other,
                            };
                            let resp = GetTokenMetadataByDenomResponse {
                                metadata: TokenMetadata {
                                    token: Token::create(token_id).unwrap(),
                                    chain_uid,
                                    token_type: token_type_with_decimals,
                                    allowed: true,
                                },
                            };
                            SystemResult::Ok(ContractResult::Ok(to_json_binary(&resp).unwrap()))
                        }
                        other => panic!("unexpected vb query: {other:?}"),
                    }
                }
                _ => panic!("unexpected wasm query"),
            });
        }

        fn make_pending(
            tx_id: &str,
            amount_in: u128,
            swap_amount: u128,
            min_lp_out: u128,
            asset_out: Token,
        ) -> RouterCrossChainSingleSidedAddLiquidityMsg {
            let chain_uid = ChainUid::create("chain1".to_string()).unwrap();
            let pair = Pair::new(token_aaa(), asset_out.clone()).unwrap();
            RouterCrossChainSingleSidedAddLiquidityMsg {
                sender: CrossChainUser::new(chain_uid.clone(), "user".to_string()),
                asset_in: TokenWithDenom {
                    token: token_aaa(),
                    token_type: TokenType::Native {
                        denom: "uaaa".to_string(),
                        decimals: None,
                    },
                },
                amount_in: Uint256::from(amount_in),
                swap_amount: Uint256::from(swap_amount),
                pair,
                swaps: vec![NextSwapPair {
                    token_in: token_aaa(),
                    token_out: asset_out,
                    pool_key: None,
                    test_fail: None,
                }],
                min_lp_out: Uint256::from(min_lp_out),
                partner_fee_amount: Uint256::zero(),
                partner_fee_recipient: CrossChainUser::new(chain_uid, "user".to_string()),
                tx_id: tx_id.to_string(),
            }
        }

        fn seed_pending(deps: &mut MockDeps, pending: &RouterCrossChainSingleSidedAddLiquidityMsg) {
            PENDING_SINGLE_SIDED_LIQUIDITY
                .save(deps.as_mut().storage, pending.tx_id.clone(), pending)
                .unwrap();
        }

        // ----------------- on_single_sided_swap_reply -----------------

        #[test]
        fn test_single_sided_swap_reply_err_returns_reply_error() {
            let mut deps = initialized();
            let reply = err_reply(SINGLE_SIDED_SWAP_REPLY_ID, "swap failed");
            let err = on_single_sided_swap_reply(deps.as_mut(), reply).unwrap_err();
            assert!(matches!(err, euclid::error::ContractError::Reply { .. }));
        }

        #[test]
        fn test_single_sided_swap_reply_missing_pending_returns_error() {
            let mut deps = initialized();
            seed_virtual_balance(&mut deps);
            seed_vlp(&mut deps);
            install_metadata_querier(&mut deps);

            // No PENDING_SINGLE_SIDED_LIQUIDITY entry seeded.
            let vlp_resp = VlpSwapResponse {
                sender: CrossChainUser::new(
                    ChainUid::create("chain1".to_string()).unwrap(),
                    "user".to_string(),
                ),
                tx_id: "tx-no-pending".to_string(),
                asset_out: token_bbb(),
                amount_out: Uint256::from(300u128),
            };
            let inner_json = cosmwasm_std::to_json_binary(&vlp_resp).unwrap();
            let proto = encode_execute_response(&inner_json);
            let reply = ok_reply(SINGLE_SIDED_SWAP_REPLY_ID, proto);

            assert!(on_single_sided_swap_reply(deps.as_mut(), reply).is_err());
        }

        #[test]
        fn test_single_sided_swap_reply_asset_out_mismatch_returns_error() {
            let mut deps = initialized();
            seed_virtual_balance(&mut deps);
            seed_vlp(&mut deps);
            install_metadata_querier(&mut deps);

            let tx_id = "tx-mismatch";
            let pending = make_pending(tx_id, 1000, 400, 10, token_bbb());
            seed_pending(&mut deps, &pending);

            // VLP returns ccc instead of bbb
            let wrong = Token::create("ccc".to_string()).unwrap();
            let vlp_resp = VlpSwapResponse {
                sender: pending.sender.clone(),
                tx_id: tx_id.to_string(),
                asset_out: wrong,
                amount_out: Uint256::from(300u128),
            };
            let inner_json = cosmwasm_std::to_json_binary(&vlp_resp).unwrap();
            let proto = encode_execute_response(&inner_json);
            let reply = ok_reply(SINGLE_SIDED_SWAP_REPLY_ID, proto);

            let err = on_single_sided_swap_reply(deps.as_mut(), reply).unwrap_err();
            assert_eq!(
                err,
                euclid::error::ContractError::new("asset_out mismatch on single-sided swap reply")
            );
        }

        #[test]
        fn test_single_sided_swap_reply_happy_path_emits_two_approvals_and_add_liq_submsg() {
            let mut deps = initialized();
            seed_virtual_balance(&mut deps);
            seed_vlp(&mut deps);
            install_metadata_querier(&mut deps);

            let tx_id = "tx-ssw-happy";
            let pending = make_pending(tx_id, 1000, 400, 10, token_bbb());
            seed_pending(&mut deps, &pending);

            let amount_out = Uint256::from(380u128);
            let vlp_resp = VlpSwapResponse {
                sender: pending.sender.clone(),
                tx_id: tx_id.to_string(),
                asset_out: token_bbb(),
                amount_out,
            };
            let inner_json = cosmwasm_std::to_json_binary(&vlp_resp).unwrap();
            let proto = encode_execute_response(&inner_json);
            let reply = ok_reply(SINGLE_SIDED_SWAP_REPLY_ID, proto);

            let res = on_single_sided_swap_reply(deps.as_mut(), reply).unwrap();

            // Expect exactly: 2 plain wasm messages (Approve in, Approve out)
            // + 1 SubMsg (AddLiquidity with SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID).
            assert_eq!(
                res.messages.len(),
                3,
                "expected 2 approvals + 1 add-liq submsg"
            );

            let mut approve_in_seen = false;
            let mut approve_out_seen = false;
            for sub in res.messages.iter().take(2) {
                if let CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) = &sub.msg {
                    let parsed: VirtualBalanceMsg = from_json(msg).unwrap();
                    match parsed {
                        VirtualBalanceMsg::Approve(a) => {
                            if a.token_id == "aaa" {
                                approve_in_seen = true;
                                // remaining_normalized = 1000 - 400 = 600 (24-dec native is identity)
                                assert_eq!(a.amount, Uint256::from(600u128));
                            } else if a.token_id == "bbb" {
                                approve_out_seen = true;
                                assert_eq!(a.amount, amount_out);
                            } else {
                                panic!("unexpected token_id in approve: {}", a.token_id);
                            }
                        }
                        VirtualBalanceMsg::Mint(_) => {
                            panic!("Mint must NOT be emitted in single-sided swap reply (would double-count)");
                        }
                        other => panic!("unexpected vb msg: {other:?}"),
                    }
                } else {
                    panic!("expected wasm execute msg");
                }
            }
            assert!(approve_in_seen, "expected Approve(asset_in)");
            assert!(approve_out_seen, "expected Approve(asset_out)");

            // Final SubMsg id matches add-liq reply id.
            assert_eq!(
                res.messages.last().unwrap().id,
                SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID
            );

            // Critical negative assertion: NO ExecuteMint anywhere in the response.
            for sub in &res.messages {
                if let CosmosMsg::Wasm(WasmMsg::Execute { msg, .. }) = &sub.msg {
                    // Only attempt to decode messages destined for the virtual_balance
                    // contract — the AddLiquidity message decodes as a vlp ExecuteMsg.
                    if let Ok(parsed) = from_json::<VirtualBalanceMsg>(msg) {
                        assert!(
                            !matches!(parsed, VirtualBalanceMsg::Mint(_)),
                            "Mint message present — would double-count asset_out"
                        );
                    }
                }
            }
        }

        // ----------------- on_single_sided_add_liquidity_reply -----------------

        #[test]
        fn test_single_sided_add_liq_reply_err_returns_reply_error() {
            let mut deps = initialized();
            let reply = err_reply(SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID, "addliq failed");
            let err = on_single_sided_add_liquidity_reply(deps.as_mut(), reply).unwrap_err();
            assert!(matches!(err, euclid::error::ContractError::Reply { .. }));
        }

        #[test]
        fn test_single_sided_add_liq_reply_missing_pending_returns_error() {
            let mut deps = initialized();

            let liq_resp = AddLiquidityResponse {
                mint_lp_tokens: Uint256::from(100u128),
                vlp_address: "vlp_contract".to_string(),
                tx_id: "tx-no-pending".to_string(),
                sender: CrossChainUser::new(
                    ChainUid::create("chain1".to_string()).unwrap(),
                    "user".to_string(),
                ),
            };
            let inner_json = cosmwasm_std::to_json_binary(&liq_resp).unwrap();
            let proto = encode_execute_response(&inner_json);
            let reply = ok_reply(SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID, proto);
            assert!(on_single_sided_add_liquidity_reply(deps.as_mut(), reply).is_err());
        }

        #[test]
        fn test_single_sided_add_liq_reply_happy_path_sets_ack_and_clears_pending() {
            let mut deps = initialized();
            let tx_id = "tx-ssal-happy";
            let pending = make_pending(tx_id, 1000, 400, 50, token_bbb());
            seed_pending(&mut deps, &pending);

            let liq_resp = AddLiquidityResponse {
                mint_lp_tokens: Uint256::from(100u128), // >= min_lp_out (50)
                vlp_address: "vlp_contract".to_string(),
                tx_id: tx_id.to_string(),
                sender: pending.sender.clone(),
            };
            let inner_json = cosmwasm_std::to_json_binary(&liq_resp).unwrap();
            let proto = encode_execute_response(&inner_json);
            let reply = ok_reply(SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID, proto);

            let res = on_single_sided_add_liquidity_reply(deps.as_mut(), reply).unwrap();

            // Ack data was set
            assert!(res.data.is_some(), "expected ack data to be set");

            // Pending entry was removed
            assert!(
                PENDING_SINGLE_SIDED_LIQUIDITY
                    .may_load(deps.as_ref().storage, tx_id.to_string())
                    .unwrap()
                    .is_none(),
                "pending entry should be cleared on success"
            );

            // Attribute sanity
            assert_eq!(
                res.attributes
                    .iter()
                    .find(|a| a.key == "action")
                    .unwrap()
                    .value,
                "single_sided_add_liquidity_reply"
            );
        }

        #[test]
        fn test_single_sided_add_liq_reply_slippage_exceeded_returns_err_and_no_set_data() {
            let mut deps = initialized();
            let tx_id = "tx-ssal-slip";
            // min_lp_out = 100, mint_lp_tokens = 50 -> SlippageExceeded
            let pending = make_pending(tx_id, 1000, 400, 100, token_bbb());
            seed_pending(&mut deps, &pending);

            let liq_resp = AddLiquidityResponse {
                mint_lp_tokens: Uint256::from(50u128),
                vlp_address: "vlp_contract".to_string(),
                tx_id: tx_id.to_string(),
                sender: pending.sender.clone(),
            };
            let inner_json = cosmwasm_std::to_json_binary(&liq_resp).unwrap();
            let proto = encode_execute_response(&inner_json);
            let reply = ok_reply(SINGLE_SIDED_ADD_LIQUIDITY_REPLY_ID, proto);

            let err = on_single_sided_add_liquidity_reply(deps.as_mut(), reply).unwrap_err();
            match err {
                euclid::error::ContractError::SlippageExceeded {
                    amount,
                    min_amount_out,
                } => {
                    assert_eq!(amount, Uint256::from(50u128));
                    assert_eq!(min_amount_out, Uint256::from(100u128));
                }
                other => panic!("expected SlippageExceeded, got {other:?}"),
            }

            // CRITICAL: when SlippageExceeded fires, the function must return Err
            // BEFORE clearing the pending state. The Err propagation is what rolls
            // back the hub state via the outer cross-chain reply. We assert here
            // that the pending entry is still present.
            assert!(
                PENDING_SINGLE_SIDED_LIQUIDITY
                    .may_load(deps.as_ref().storage, tx_id.to_string())
                    .unwrap()
                    .is_some(),
                "pending entry must remain when slippage triggers Err (rollback path)"
            );
        }
    }
}
