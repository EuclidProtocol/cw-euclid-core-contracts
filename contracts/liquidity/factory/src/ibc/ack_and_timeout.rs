#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcAcknowledgement, IbcBasicResponse,
    IbcPacketAckMsg, IbcPacketTimeoutMsg, StdResult, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    error::ContractError,
    msgs::pool::CallbackExecuteMsg,
    pool::{LiquidityResponse, Pool, PoolCreationResponse},
    swap::SwapResponse,
    token::PairInfo,
};
use euclid_ibc::msg::{AcknowledgementMsg, ChainIbcExecuteMsg};

use crate::{
    reply::INSTANTIATE_REPLY_ID,
    state::{POOL_REQUESTS, STATE},
};

use super::channel::TIMEOUT_COUNTS;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    _env: Env,
    ack: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    // Parse the ack based on request
    let msg: ChainIbcExecuteMsg = from_json(&ack.original_packet.data)?;
    match msg {
        ChainIbcExecuteMsg::RequestPoolCreation {
            pool_rq_id,
            pair_info,
            ..
        } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<PoolCreationResponse> =
                from_json(ack.acknowledgement.data)?;

            execute_pool_creation(deps, pair_info, res, pool_rq_id)
        }
        ChainIbcExecuteMsg::Swap {
            swap_id,
            pool_address,
            ..
        } => {
            // Process acknowledgment for swap
            let res: AcknowledgementMsg<SwapResponse> = from_json(ack.acknowledgement.data)?;
            execute_swap_process(res, pool_address.to_string(), swap_id)
        }

        ChainIbcExecuteMsg::AddLiquidity {
            liquidity_id,
            pool_address,
            ..
        } => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<LiquidityResponse> = from_json(ack.acknowledgement.data)?;
            execute_add_liquidity_process(res, pool_address, liquidity_id)
        }
        _ => Err(ContractError::Unauthorized {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse, ContractError> {
    TIMEOUT_COUNTS.update(
        deps.storage,
        // timed out packets are sent by us, so lookup based on packet
        // source, not destination.
        msg.packet.src.channel_id.clone(),
        |count| -> StdResult<_> { Ok(count.unwrap_or_default() + 1) },
    )?;
    let failed_ack = IbcAcknowledgement::new(to_json_binary(&AcknowledgementMsg::Error::<()>(
        "Timeout".to_string(),
    ))?);

    let failed_ack_simulation = IbcPacketAckMsg::new(failed_ack, msg.packet, msg.relayer);

    // We want to handle timeout in same way we handle failed acknowledgement
    let result = ibc_packet_ack(deps, env, failed_ack_simulation);

    result.or(Ok(
        IbcBasicResponse::new().add_attribute("method", "ibc_packet_timeout")
    ))
}

// Function to create pool
pub fn execute_pool_creation(
    deps: DepsMut,
    pair_info: PairInfo,
    res: AcknowledgementMsg<PoolCreationResponse>,
    pool_rq_id: String,
) -> Result<IbcBasicResponse, ContractError> {
    let _existing_req = POOL_REQUESTS
        .may_load(deps.storage, pool_rq_id.clone())?
        .ok_or(ContractError::PoolRequestDoesNotExists {
            req: pool_rq_id.clone(),
        })?;
    // Load the state
    let state = STATE.load(deps.storage)?;
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Check if the pool was created successfully
            // Prepare Instantiate Msg
            let init_msg = euclid::msgs::pool::InstantiateMsg {
                vlp_contract: data.vlp_contract.clone(),
                pool: Pool {
                    chain: state.chain_id.clone(),
                    pair: pair_info,
                    reserve_1: Uint128::zero(),
                    reserve_2: Uint128::zero(),
                },
                chain_id: state.chain_id.clone(),
            };

            let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                admin: None,
                code_id: state.pool_code_id,
                msg: to_json_binary(&init_msg)?,
                funds: vec![],
                label: "euclid-pool".to_string(),
            });

            // Create submsg with reply always from msg
            let msg: SubMsg = SubMsg::reply_always(msg, INSTANTIATE_REPLY_ID);
            // Remove pool request from MAP
            POOL_REQUESTS.remove(deps.storage, pool_rq_id);
            Ok(IbcBasicResponse::new()
                .add_attribute("method", "pool_creation")
                .add_submessage(msg))
        }

        AcknowledgementMsg::Error(err) => {
            // Remove pool request from MAP
            POOL_REQUESTS.remove(deps.storage, pool_rq_id);
            Ok(IbcBasicResponse::new()
                .add_attribute("method", "refund_pool_request")
                .add_attribute("error", err.clone()))
        }
    }
}

// Function to process swap acknowledgment
pub fn execute_swap_process(
    res: AcknowledgementMsg<SwapResponse>,
    pool_address: String,
    swap_id: String,
) -> Result<IbcBasicResponse, ContractError> {
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Prepare callback to send to pool
            let callback = CallbackExecuteMsg::CompleteSwap {
                swap_response: data.clone(),
            };
            let msg = euclid::msgs::pool::ExecuteMsg::Callback(callback);

            let execute = WasmMsg::Execute {
                contract_addr: pool_address.clone(),
                msg: to_json_binary(&msg.clone())?,
                funds: vec![],
            };

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "swap")
                .add_message(execute))
        }

        AcknowledgementMsg::Error(err) => {
            // Prepare error callback to send to pool
            let callback = CallbackExecuteMsg::RejectSwap {
                swap_id: swap_id.clone(),
                error: Some(err.clone()),
            };

            let msg = euclid::msgs::pool::ExecuteMsg::Callback(callback);
            let execute = WasmMsg::Execute {
                contract_addr: pool_address.clone(),
                msg: to_json_binary(&msg.clone())?,
                funds: vec![],
            };

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "swap")
                .add_attribute("error", err.clone())
                .add_message(execute))
        }
    }
}

// Function to process add liquidity acknowledgment
pub fn execute_add_liquidity_process(
    res: AcknowledgementMsg<LiquidityResponse>,
    pool_address: String,
    liquidity_id: String,
) -> Result<IbcBasicResponse, ContractError> {
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Prepare callback to send to pool
            let callback = CallbackExecuteMsg::CompleteAddLiquidity {
                liquidity_response: data.clone(),
                liquidity_id: liquidity_id.clone(),
            };
            let msg = euclid::msgs::pool::ExecuteMsg::Callback(callback);

            let execute = WasmMsg::Execute {
                contract_addr: pool_address.clone(),
                msg: to_json_binary(&msg.clone())?,
                funds: vec![],
            };

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "add_liquidity")
                .add_message(execute))
        }

        AcknowledgementMsg::Error(err) => {
            // Prepare error callback to send to pool
            let callback = CallbackExecuteMsg::RejectAddLiquidity {
                liquidity_id,
                error: Some(err.clone()),
            };

            let msg = euclid::msgs::pool::ExecuteMsg::Callback(callback);
            let execute = WasmMsg::Execute {
                contract_addr: pool_address.clone(),
                msg: to_json_binary(&msg.clone())?,
                funds: vec![],
            };

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "add_liquidity")
                .add_attribute("error", err.clone())
                .add_message(execute))
        }
    }
}
