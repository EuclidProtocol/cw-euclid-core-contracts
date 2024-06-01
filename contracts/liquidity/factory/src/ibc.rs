use std::vec;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcChannel,
    IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcChannelOpenResponse, IbcOrder,
    IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse, StdResult,
    SubMsg, Uint128, WasmMsg,
};
use euclid::msgs::pool::ExecuteMsg as PoolExecuteMsg;
use euclid::token::PairInfo;
use euclid::{
    error::ContractError,
    msgs::pool::CallbackExecuteMsg,
    pool::{LiquidityResponse, Pool, PoolCreationResponse},
    swap::SwapResponse,
};
use euclid_ibc::ack::make_ack_fail;
use euclid_ibc::msg::{AcknowledgementMsg, IbcExecuteMsg};

use crate::{
    reply::INSTANTIATE_REPLY_ID,
    state::{CONNECTION_COUNTS, POOL_REQUESTS, STATE, TIMEOUT_COUNTS},
};

use euclid::msgs::pool::InstantiateMsg as PoolInstantiateMsg;

pub const IBC_VERSION: &str = "counter-1";

/// Handles the `OpenInit` and `OpenTry` parts of the IBC handshake.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_open(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelOpenMsg,
) -> Result<IbcChannelOpenResponse, ContractError> {
    validate_order_and_version(msg.channel(), msg.counterparty_version())?;
    Ok(None)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_connect(
    deps: DepsMut,
    _env: Env,
    msg: IbcChannelConnectMsg,
) -> Result<IbcBasicResponse, ContractError> {
    validate_order_and_version(msg.channel(), msg.counterparty_version())?;

    // Initialize the count for this channel to zero.
    let channel = msg.channel().endpoint.channel_id.clone();
    CONNECTION_COUNTS.save(deps.storage, channel.clone(), &0)?;

    Ok(IbcBasicResponse::new()
        .add_attribute("method", "ibc_channel_connect")
        .add_attribute("channel_id", channel))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_close(
    deps: DepsMut,
    _env: Env,
    msg: IbcChannelCloseMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let channel = msg.channel().endpoint.channel_id.clone();
    // Reset the state for the channel.
    CONNECTION_COUNTS.remove(deps.storage, channel.clone());
    Ok(IbcBasicResponse::new()
        .add_attribute("method", "ibc_channel_close")
        .add_attribute("channel", channel))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // Regardless of if our processing of this packet works we need to
    // commit an ACK to the chain. As such, we wrap all handling logic
    // in a seprate function and on error write out an error ack.
    match do_ibc_packet_receive(deps, env, msg) {
        Ok(response) => Ok(response),
        Err(error) => Ok(IbcReceiveResponse::new()
            .add_attribute("method", "ibc_packet_receive")
            .add_attribute("error", error.to_string())
            .set_ack(make_ack_fail(error.to_string())?)),
    }
}

pub fn do_ibc_packet_receive(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // Pool does not handle any IBC Packets
    Ok(IbcReceiveResponse::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    _env: Env,
    ack: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    // Parse the ack based on request
    let msg: IbcExecuteMsg = from_json(&ack.original_packet.data)?;
    match msg {
        IbcExecuteMsg::RequestPoolCreation {
            pool_rq_id,
            pair_info,
            ..
        } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<PoolCreationResponse> =
                from_json(ack.acknowledgement.data)?;

            execute_pool_creation(deps, pair_info, res, pool_rq_id)
        }
        IbcExecuteMsg::Swap {
            swap_id,
            pool_address,
            ..
        } => {
            // Process acknowledgment for swap
            let res: AcknowledgementMsg<SwapResponse> = from_json(ack.acknowledgement.data)?;
            execute_swap_process(res, pool_address.to_string(), swap_id)
        }

        IbcExecuteMsg::AddLiquidity {
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
    _env: Env,
    msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse, ContractError> {
    TIMEOUT_COUNTS.update(
        deps.storage,
        // timed out packets are sent by us, so lookup based on packet
        // source, not destination.
        msg.packet.src.channel_id,
        |count| -> StdResult<_> { Ok(count.unwrap_or_default() + 1) },
    )?;
    let parsed_msg: IbcExecuteMsg = from_json(&msg.packet.data)?;

    let result = match parsed_msg {
        IbcExecuteMsg::AddLiquidity {
            liquidity_id,
            pool_address,
            ..
        } => {
            let fake_error_ack = AcknowledgementMsg::Error("Timeout".to_string());
            execute_add_liquidity_process(fake_error_ack, pool_address, liquidity_id)
        }
        IbcExecuteMsg::Swap {
            swap_id,
            pool_address,
            ..
        } => {
            let fake_error_ack = AcknowledgementMsg::Error("Timeout".to_string());
            execute_swap_process(fake_error_ack, pool_address.to_string(), swap_id)
        }
        IbcExecuteMsg::RequestPoolCreation {
            pool_rq_id,
            pair_info,
            ..
        } => {
            let fake_error_ack = AcknowledgementMsg::Error("Timeout".to_string());
            execute_pool_creation(deps, pair_info, fake_error_ack, pool_rq_id)
        }
        _ => Ok(IbcBasicResponse::new().add_attribute("method", "ibc_packet_timeout")),
    };
    result.or(Ok(
        IbcBasicResponse::new().add_attribute("method", "ibc_packet_timeout")
    ))
}

pub fn validate_order_and_version(
    channel: &IbcChannel,
    counterparty_version: Option<&str>,
) -> Result<(), ContractError> {
    // We expect an unordered channel here. Ordered channels have the
    // property that if a message is lost the entire channel will stop
    // working until you start it again.
    ensure!(
        channel.order == IbcOrder::Unordered,
        ContractError::OrderedChannel {}
    );

    ensure!(
        channel.version == IBC_VERSION,
        ContractError::InvalidVersion {
            actual: channel.version.to_string(),
            expected: IBC_VERSION.to_string(),
        }
    );

    // Make sure that we're talking with a counterparty who speaks the
    // same "protocol" as us.
    //
    // For a connection between chain A and chain B being established
    // by chain A, chain B knows counterparty information during
    // `OpenTry` and chain A knows counterparty information during
    // `OpenAck`. We verify it when we have it but when we don't it's
    // alright.
    if let Some(counterparty_version) = counterparty_version {
        ensure!(
            counterparty_version == IBC_VERSION,
            ContractError::InvalidVersion {
                actual: counterparty_version.to_string(),
                expected: IBC_VERSION.to_string(),
            }
        );
    }

    Ok(())
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
            let init_msg = PoolInstantiateMsg {
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
            let msg = PoolExecuteMsg::Callback(callback);

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

            let msg = PoolExecuteMsg::Callback(callback);
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
            let msg = PoolExecuteMsg::Callback(callback);

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

            let msg = PoolExecuteMsg::Callback(callback);
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
