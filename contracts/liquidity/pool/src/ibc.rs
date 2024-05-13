
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcChannel, IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcChannelOpenResponse, IbcOrder, IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse, StdResult
};
use euclid::{error::{ContractError, Never}, swap::extract_sender};
use euclid_ibc::msg::{AcknowledgementMsg, IbcExecuteMsg, LiquidityResponse, SwapResponse};

use crate::{
    ack::make_ack_fail, state::{get_liquidity_info, get_swap_info, CONNECTION_COUNTS, PENDING_LIQUIDITY, PENDING_SWAPS, STATE, TIMEOUT_COUNTS}
};

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
) -> Result<IbcReceiveResponse, Never> {
    // Regardless of if our processing of this packet works we need to
    // commit an ACK to the chain. As such, we wrap all handling logic
    // in a seprate function and on error write out an error ack.
    match do_ibc_packet_receive(deps, env, msg) {
        Ok(response) => Ok(response),
        Err(error) => Ok(IbcReceiveResponse::new()
            .add_attribute("method", "ibc_packet_receive")
            .add_attribute("error", error.to_string())
            .set_ack(make_ack_fail(error.to_string()))),
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
        IbcExecuteMsg::Swap {swap_id, chain_id, ..} => {
            let state = STATE.load(deps.storage)?;
            ensure!(
                state.chain_id == chain_id,
                ContractError::InvalidChainId { }
            );

            let res: AcknowledgementMsg<SwapResponse> = from_json(ack.acknowledgement.data)?;
            execute_swap(deps, res, swap_id)
    },

        IbcExecuteMsg::AddLiquidity { chain_id, liquidity_id, .. } => {
            // Verify Chain ID same as in state
            let state = STATE.load(deps.storage)?;
            ensure!(
                state.chain_id == chain_id,
                ContractError::InvalidChainId { }
            );
            let res: AcknowledgementMsg<LiquidityResponse> = from_json(ack.acknowledgement.data)?;
            execute_liquidity_ack(deps, res, liquidity_id)
        },

        IbcExecuteMsg:: RemoveLiquidity { .. } => {
            Ok(IbcBasicResponse::new())
        }
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
    
    let original_msg: IbcExecuteMsg = from_json(&msg.packet.data)?;

    // Match message to handle timeout
    match original_msg {
        IbcExecuteMsg::Swap { swap_id,  .. } => {
        // On timeout, we need to refund the tokens back to the sender as swap is impossible to be completed
        // Fetch the sender from swap_id
        let sender = extract_sender(&swap_id);
        // Fetch the pending swaps for the sender 
        let pending_swaps = PENDING_SWAPS.load(deps.storage, sender.clone())?;
        // Get the current pending swap
        let swap_info = get_swap_info(&swap_id, pending_swaps.clone());
        // Pop this swap from the vector
        let mut new_pending_swaps = pending_swaps.clone();
        new_pending_swaps.retain(|x| x.swap_id != swap_id);
        // Update the pending swaps
        PENDING_SWAPS.save(deps.storage, sender.clone(), &new_pending_swaps)?;

        // Prepare messages to refund tokens back to user
        let msg = swap_info.asset.create_transfer_msg(swap_info.asset_amount, sender.clone());
        Ok(IbcBasicResponse::new()
        .add_attribute("method", "handle_swap_timeout")
        .add_attribute("sender", sender.clone())
        .add_attribute("swap_id", swap_id.clone())
        .add_message(msg))
        },
    
        IbcExecuteMsg::AddLiquidity { liquidity_id, .. } => {
            // Commit Refund to sender
            // Fetch the sender from liquidity_id
            let state = STATE.load(deps.storage)?;
            // Fetch the 2 tokens
            let token_1 = state.pair_info.token_1;
            let token_2 = state.pair_info.token_2;
            let sender = extract_sender(&liquidity_id);
            // Fetch the pending liquidity transactions for the sender
            let pending_liquidity = PENDING_LIQUIDITY.load(deps.storage, sender.clone())?;
            // Get the current pending liquidity transaction
            let liquidity_info = get_liquidity_info(&liquidity_id, pending_liquidity.clone());
            // Pop this liquidity transaction from the vector
            let mut new_pending_liquidity = pending_liquidity.clone();
            new_pending_liquidity.retain(|x| x.liquidity_id != liquidity_id);
            // Update the pending liquidity transactions
            PENDING_LIQUIDITY.save(deps.storage, sender.clone(), &new_pending_liquidity)?;

            // Prepare messages to refund tokens back to user
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            let msg = token_1.clone().create_transfer_msg(liquidity_info.token_1_liquidity, sender.clone());
            msgs.push(msg);
            let msg = token_2.clone().create_transfer_msg(liquidity_info.token_2_liquidity, sender.clone());
            msgs.push(msg);


            Ok(IbcBasicResponse::new()
            .add_attribute("method", "liquidity_tx_timeout")
            .add_attribute("sender", sender.clone())
            .add_attribute("liquidity_id", liquidity_id.clone())
            .add_messages(msgs)
        )
        },

        IbcExecuteMsg:: RemoveLiquidity { .. } => {
            Ok(IbcBasicResponse::new())
        }
    }
   
}

pub fn validate_order_and_version(
    channel: &IbcChannel,
    counterparty_version: Option<&str>,
) -> Result<(), ContractError> {
    // We expect an unordered channel here. Ordered channels have the
    // property that if a message is lost the entire channel will stop
    // working until you start it again.
    if channel.order != IbcOrder::Unordered {
        return Err(ContractError::OrderedChannel {});
    }

    if channel.version != IBC_VERSION {
        return Err(ContractError::InvalidVersion {
            actual: channel.version.to_string(),
            expected: IBC_VERSION.to_string(),
        });
    }

    // Make sure that we're talking with a counterparty who speaks the
    // same "protocol" as us.
    //
    // For a connection between chain A and chain B being established
    // by chain A, chain B knows counterparty information during
    // `OpenTry` and chain A knows counterparty information during
    // `OpenAck`. We verify it when we have it but when we don't it's
    // alright.
    if let Some(counterparty_version) = counterparty_version {
        if counterparty_version != IBC_VERSION {
            return Err(ContractError::InvalidVersion {
                actual: counterparty_version.to_string(),
                expected: IBC_VERSION.to_string(),
            });
        }
    }

    Ok(())
}


// Function execute_swap that routes the swap request to the appropriate function
pub fn execute_swap(
    deps: DepsMut,
    ack: AcknowledgementMsg<SwapResponse>,
    swap_id: String) -> Result<IbcBasicResponse, ContractError> {
        // Parse the ack based on request
        match ack {
            AcknowledgementMsg::Ok(resp) => {
                // Unpack response
        // Verify that both assets exist in state
        let mut state = STATE.load(deps.storage)?;
        
        // Verify that assets exist in the state.
        ensure!(
            resp.asset.exists(state.clone().pair),
            ContractError::AssetDoesNotExist {  }
        );

        ensure!(
            resp.asset_out.exists(state.clone().pair),
            ContractError::AssetDoesNotExist {  }
        );
        
        // Fetch the sender from swap_id
        let sender = extract_sender(&resp.swap_id);

        // Validate that the pending swap exists for the sender
        let pending_swaps = PENDING_SWAPS.load(deps.storage, sender.clone())?;

        // Get swap id info
        let swap_info = get_swap_info(&resp.swap_id, pending_swaps.clone());

        // Pop the swap from the pending swaps
        let mut new_pending_swaps = pending_swaps.clone();
        new_pending_swaps.retain(|x| x.swap_id != resp.swap_id);

        // Update the pending swaps
        PENDING_SWAPS.save(deps.storage, sender.clone(), &new_pending_swaps)?;

        // Check if asset is token_1 or token_2 and calculate accordingly
        if resp.asset == state.clone().pair.token_1 {
            state.reserve_1 += resp.asset_amount;
            state.reserve_2 -= resp.amount_out;

        } else {
            state.reserve_2 += resp.asset_amount;
            state.reserve_1 -= resp.amount_out;
        };

        // Save the updated state
        STATE.save(deps.storage, &state)?;

        // Prepare messages to send tokens to user
        let msg = swap_info.asset_out.create_transfer_msg(resp.amount_out, sender);

        // Look through pending swaps for one with the same swap_id
        Ok(IbcBasicResponse::new()
        .add_message(msg))
            },

        // If acknowledgment is an error, the refund proccess should take place
        AcknowledgementMsg::Error(e) => {
                // Fetch the sender from swap_id
                let sender = extract_sender(&swap_id);
                // Fetch the pending swaps for the sender 
                let pending_swaps = PENDING_SWAPS.load(deps.storage, sender.clone())?;
                // Get the current pending swap
                let swap_info = get_swap_info(&swap_id, pending_swaps.clone());
                // Pop this swap from the vector
                let mut new_pending_swaps = pending_swaps.clone();
                new_pending_swaps.retain(|x| x.swap_id != swap_id);
                // Update the pending swaps
                PENDING_SWAPS.save(deps.storage, sender.clone(), &new_pending_swaps)?;
        
                // Prepare messages to refund tokens back to user
                let msg = swap_info.asset.create_transfer_msg(swap_info.asset_amount, sender.clone());
        
                Ok(IbcBasicResponse::new()
                .add_attribute("method", "process_failed_swap")
                .add_attribute("refund_to", "sender")
                .add_attribute("refund_amount", swap_info.asset_amount.clone())
                .add_attribute("error", e)
                .add_message(msg))
            }
        }
    }

// Function to execute after LiquidityResponse acknowledgment
pub fn execute_liquidity_ack(
    deps: DepsMut,
    ack: AcknowledgementMsg<LiquidityResponse>,
    liquidity_id: String) -> Result<IbcBasicResponse, ContractError> {
        match ack {
            AcknowledgementMsg::Ok(resp) => {
                // Unpack response
                // Fetch the sender from liquidity_id
                let sender = extract_sender(&liquidity_id);
                // Fetch the pending liquidity transactions for the sender
                let pending_liquidity = PENDING_LIQUIDITY.load(deps.storage, sender.clone())?;
                // Get the current pending liquidity transaction
                let _liquidity_info = get_liquidity_info(&liquidity_id, pending_liquidity.clone());
                // Pop this liquidity transaction from the vector
                let mut new_pending_liquidity = pending_liquidity.clone();
                new_pending_liquidity.retain(|x| x.liquidity_id != liquidity_id);
                // Update the pending liquidity transactions
                PENDING_LIQUIDITY.save(deps.storage, sender.clone(), &new_pending_liquidity)?;

                // Update the state with the new reserves
                let mut state = STATE.load(deps.storage)?;
                state.reserve_1 += resp.token_1_liquidity;
                state.reserve_2 += resp.token_2_liquidity;

                // Save the updated state
                STATE.save(deps.storage, &state)?;

                Ok(IbcBasicResponse::new()
                .add_attribute("method", "process_add_liquidity")
                .add_attribute("sender", sender.clone())
                .add_attribute("liquidity_id", liquidity_id.clone())
            )

            },
            // If error, process refund
            AcknowledgementMsg::Error(e) => {

            let state = STATE.load(deps.storage)?;
            // Fetch the 2 tokens
            let token_1 = state.pair_info.token_1;
            let token_2 = state.pair_info.token_2;
            let sender = extract_sender(&liquidity_id);
            // Fetch the pending liquidity transactions for the sender
            let pending_liquidity = PENDING_LIQUIDITY.load(deps.storage, sender.clone())?;
            // Get the current pending liquidity transaction
            let liquidity_info = get_liquidity_info(&liquidity_id, pending_liquidity.clone());
            // Pop this liquidity transaction from the vector
            let mut new_pending_liquidity = pending_liquidity.clone();
            new_pending_liquidity.retain(|x| x.liquidity_id != liquidity_id);
            // Update the pending liquidity transactions
            PENDING_LIQUIDITY.save(deps.storage, sender.clone(), &new_pending_liquidity)?;

            // Prepare messages to refund tokens back to user
            let mut msgs: Vec<CosmosMsg> = Vec::new();
            let msg = token_1.clone().create_transfer_msg(liquidity_info.token_1_liquidity, sender.clone());
            msgs.push(msg);
            let msg = token_2.clone().create_transfer_msg(liquidity_info.token_2_liquidity, sender.clone());
            msgs.push(msg);


            Ok(IbcBasicResponse::new()
            .add_attribute("method", "liquidity_tx_err_refund")
            .add_attribute("sender", sender.clone())
            .add_attribute("liquidity_id", liquidity_id.clone())
            .add_attribute("error", e)
            .add_messages(msgs)
        )
            }
            

        }
    }
