#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, CosmosMsg, DepsMut, Env, IbcAcknowledgement, IbcBasicResponse,
    IbcPacketAckMsg, IbcPacketTimeoutMsg, StdResult, SubMsg, Uint128, WasmMsg,
};
use euclid::{
    error::ContractError,
    liquidity,
    pool::{InstantiateEscrowResponse, LiquidityResponse, PoolCreationResponse, WithdrawResponse},
    swap::{self, SwapResponse},
    token::{PairInfo, Token},
};
use euclid_ibc::msg::{AcknowledgementMsg, ChainIbcExecuteMsg};

use crate::state::{
    PENDING_LIQUIDITY, PENDING_SWAPS, POOL_REQUESTS, STATE, TOKEN_TO_ESCROW, VLP_TO_POOL,
};

use super::channel::TIMEOUT_COUNTS;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    env: Env,
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

            ack_pool_creation(deps, pair_info, res, pool_rq_id)
        }

        ChainIbcExecuteMsg::AddLiquidity {
            liquidity_id,
            pool_address,
            ..
        } => {
            // Process acknowledgment for add liquidity
            let res: AcknowledgementMsg<LiquidityResponse> = from_json(ack.acknowledgement.data)?;
            ack_add_liquidity(deps, res, liquidity_id)
        }
        ChainIbcExecuteMsg::Swap { swap_id, .. } => {
            // Process acknowledgment for swap
            let res: AcknowledgementMsg<SwapResponse> = from_json(ack.acknowledgement.data)?;
            ack_swap_request(deps, res, swap_id)
        }
        ChainIbcExecuteMsg::RequestWithdraw { token_id, .. } => {
            let res: AcknowledgementMsg<WithdrawResponse> = from_json(ack.acknowledgement.data)?;
            ack_request_withdraw(deps, res, token_id)
        }
        ChainIbcExecuteMsg::RequestEscrowCreation { token_id } => {
            let res: AcknowledgementMsg<InstantiateEscrowResponse> =
                from_json(ack.acknowledgement.data)?;
            ack_request_instantiate_escrow(deps, env, res, token_id)
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
// TODO change this function
pub fn ack_pool_creation(
    deps: DepsMut,
    pair_info: PairInfo,
    res: AcknowledgementMsg<PoolCreationResponse>,
    pool_rq_id: String,
) -> Result<IbcBasicResponse, ContractError> {
    let existing_req = POOL_REQUESTS
        .may_load(deps.storage, pool_rq_id.clone())?
        .ok_or(ContractError::PoolRequestDoesNotExists {
            req: pool_rq_id.clone(),
        })?;
    // Load the state
    let state = STATE.load(deps.storage)?;
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Remove pool request from MAP
            POOL_REQUESTS.remove(deps.storage, pool_rq_id);

            VLP_TO_POOL.save(
                deps.storage,
                data.vlp_contract.clone(),
                &existing_req.pair_info,
            )?;
            Ok(IbcBasicResponse::new()
                .add_attribute("method", "pool_creation")
                .add_attribute("vlp", data.vlp_contract))
        }

        AcknowledgementMsg::Error(err) => {
            // Remove pool request from MAP
            POOL_REQUESTS.remove(deps.storage, pool_rq_id);
            Ok(IbcBasicResponse::new()
                .add_attribute("method", "reject_pool_request")
                .add_attribute("error", err.clone()))
        }
    }
}

// Function to process swap acknowledgment
// TODO this needs to be changed, callback msgs should probably sent to escrow
pub fn ack_swap_request(
    deps: DepsMut,
    res: AcknowledgementMsg<SwapResponse>,
    swap_id: String,
) -> Result<IbcBasicResponse, ContractError> {
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            let extracted_swap_id = swap::parse_swap_id(&swap_id)?;

            // Validate that the pending swap exists for the sender
            let swap_info = PENDING_SWAPS.load(
                deps.storage,
                (extracted_swap_id.sender.clone(), extracted_swap_id.index),
            )?;
            // Remove this from pending swaps
            PENDING_SWAPS.remove(
                deps.storage,
                (extracted_swap_id.sender.clone(), extracted_swap_id.index),
            );

            // TODO:: Add msg to send these funds to escrow

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "process_successfull_swap")
                .add_attribute("swap_response", format!("{data:?}")))
        }

        AcknowledgementMsg::Error(err) => {
            let extracted_swap_id = swap::parse_swap_id(&swap_id)?;

            // Validate that the pending swap exists for the sender
            let swap_info = PENDING_SWAPS.load(
                deps.storage,
                (extracted_swap_id.sender.clone(), extracted_swap_id.index),
            )?;
            // Remove this from pending swaps
            PENDING_SWAPS.remove(
                deps.storage,
                (extracted_swap_id.sender.clone(), extracted_swap_id.index),
            );

            // Prepare messages to refund tokens back to user
            let msg = swap_info
                .asset_in
                .create_transfer_msg(swap_info.amount_in, extracted_swap_id.sender)?;

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "process_failed_swap")
                .add_attribute("refund_to", "sender")
                .add_attribute("refund_amount", swap_info.amount_in)
                .add_attribute("error", err)
                .add_message(msg))
        }
    }
}

// Function to process add liquidity acknowledgment
pub fn ack_add_liquidity(
    deps: DepsMut,
    res: AcknowledgementMsg<LiquidityResponse>,
    liquidity_id: String,
) -> Result<IbcBasicResponse, ContractError> {
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            let extracted_liquidity_id = liquidity::parse_liquidity_id(&liquidity_id)?;

            // Validate that the pending exists for the sender
            let liquidity_info = PENDING_LIQUIDITY.load(
                deps.storage,
                (
                    extracted_liquidity_id.sender.clone(),
                    extracted_liquidity_id.index,
                ),
            )?;
            // Remove this from pending
            PENDING_LIQUIDITY.remove(
                deps.storage,
                (
                    extracted_liquidity_id.sender.clone(),
                    extracted_liquidity_id.index,
                ),
            );

            // TODO:: Add message to send these funds to escrow

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "process_add_liquidity")
                .add_attribute("sender", extracted_liquidity_id.sender)
                .add_attribute("liquidity_id", liquidity_id.clone())
                .add_attribute("pending_liquidity", format!("{liquidity_info:?}")))
        }

        AcknowledgementMsg::Error(err) => {
            let extracted_liquidity_id = liquidity::parse_liquidity_id(&liquidity_id)?;

            // Validate that the pending exists for the sender
            let liquidity_info = PENDING_LIQUIDITY.load(
                deps.storage,
                (
                    extracted_liquidity_id.sender.clone(),
                    extracted_liquidity_id.index,
                ),
            )?;
            // Remove this from pending
            PENDING_LIQUIDITY.remove(
                deps.storage,
                (
                    extracted_liquidity_id.sender.clone(),
                    extracted_liquidity_id.index,
                ),
            );

            // Prepare messages to refund tokens back to user
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            let msg = liquidity_info.pair_info.token_1.create_transfer_msg(
                liquidity_info.token_1_liquidity,
                extracted_liquidity_id.sender.clone(),
            )?;
            msgs.push(msg);
            let msg = liquidity_info.pair_info.token_1.create_transfer_msg(
                liquidity_info.token_2_liquidity,
                extracted_liquidity_id.sender.clone(),
            )?;
            msgs.push(msg);

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "liquidity_tx_err_refund")
                .add_attribute("sender", extracted_liquidity_id.sender)
                .add_attribute("liquidity_id", liquidity_id.clone())
                .add_attribute("error", err)
                .add_messages(msgs))
        }
    }
}

// New factory functions
pub fn ack_request_withdraw(
    deps: DepsMut,
    res: AcknowledgementMsg<WithdrawResponse>,
    token_id: Token,
) -> Result<IbcBasicResponse, ContractError> {
    match res {
        AcknowledgementMsg::Ok(_) => {
            let escrow_address = TOKEN_TO_ESCROW
                .load(deps.storage, token_id.clone())
                .map_err(|_err| ContractError::EscrowDoesNotExist {})?;

            // Use it for logging, Router will send packets instead of ack to release tokens from escrow
            // Here you will get a response of escrows that router is going to release so it can be used in frontend

            Ok(IbcBasicResponse::new()
                .add_attribute("method", "request_withdraw_submitted")
                .add_attribute("token", token_id.id))
        }
        AcknowledgementMsg::Error(err) => Ok(IbcBasicResponse::new()
            .add_attribute("method", "request_withdraw_error")
            .add_attribute("error", err.clone())),
    }
}

pub fn ack_request_instantiate_escrow(
    deps: DepsMut,
    env: Env,
    res: AcknowledgementMsg<InstantiateEscrowResponse>,
    token_id: Token,
) -> Result<IbcBasicResponse, ContractError> {
    match res {
        AcknowledgementMsg::Ok(data) => {
            let escrow_address = TOKEN_TO_ESCROW.may_load(deps.storage, token_id.clone())?;
            match escrow_address {
                Some(escrow_address) => Err(ContractError::EscrowAlreadyExists {}),
                None => {
                    let msg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                        admin: Some(env.contract.address.into_string()),
                        code_id: data.escrow_code_id,
                        msg: to_json_binary(&euclid::msgs::escrow::InstantiateMsg {
                            token_id: token_id.clone(),
                        })?,
                        funds: vec![],
                        label: "".to_string(),
                    });
                    Ok(IbcBasicResponse::new()
                        .add_submessage(SubMsg::new(msg))
                        .add_attribute("method", "instantiate_escrow")
                        .add_attribute("token", token_id.id))
                }
            }
        }
        AcknowledgementMsg::Error(err) => Ok(IbcBasicResponse::new()
            .add_attribute("method", "instantiate_escrow")
            .add_attribute("error", err.clone())),
    }
}
