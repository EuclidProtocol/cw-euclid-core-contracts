#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, from_slice, DepsMut, Env, IbcBasicResponse, IbcChannel, IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcChannelOpenResponse, IbcOrder, IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse, StdResult, Uint128
};
use euclid::{error::{ContractError, Never}, swap::{self, extract_sender}, token::{PairInfo, Token}};
use euclid_ibc::msg::{AcknowledgementMsg, IbcExecuteMsg, SwapResponse};

use crate::{
    ack::make_ack_fail, contract::execute, state::{find_swap_id, get_swap_info, CONNECTION_COUNTS, PENDING_SWAPS, STATE, TIMEOUT_COUNTS}
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
    deps: DepsMut,
    _env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    // Pool does not handle any IBC Packets
    Ok(IbcReceiveResponse::default())
}


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    env: Env,
    ack: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    // Parse the ack based on request
    let msg: IbcExecuteMsg = from_json(&ack.original_packet.data)?;
    
    match msg {
        IbcExecuteMsg::Swap { chain_id, asset, asset_amount, min_amount_out, channel, swap_id } => {
            let processed_ack: AcknowledgementMsg<SwapResponse> = from_json(ack.acknowledgement.data)?;
            match processed_ack {
                AcknowledgementMsg::Ok(a) => execute_success_swap(deps,env,a),
                AcknowledgementMsg::Error(e) => process_failed_swap(deps, asset, asset_amount,swap_id),
            };
    },

        IbcExecuteMsg::AddLiquidity { chain_id, token_1_liquidity, token_2_liquidity, slippage_tolerance } => {
            
        },

        IbcExecuteMsg:: RemoveLiquidity { chain_id, lp_allocation } => {

        }
    }
    Ok(IbcBasicResponse::new().add_attribute("method", "ibc_packet_ack"))
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
    // As with ack above, nothing to do here. If we cared about
    // keeping track of state between the two chains then we'd want to
    // respond to this likely as it means that the packet in question
    // isn't going anywhere.
    Ok(IbcBasicResponse::new().add_attribute("method", "ibc_packet_timeout"))
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

// Function that proceses a successful swap performed on the VLP
pub fn execute_success_swap(
    deps: DepsMut,
    _env: Env,
    resp: SwapResponse) -> Result<IbcReceiveResponse,ContractError> {
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
        Ok(IbcReceiveResponse::new().add_message(msg))
        }

// Function that processes a failed swap
pub fn process_failed_swap(
    deps: DepsMut,
    asset: Token,
    asset_amount: Uint128,
    swap_id: String) -> Result<IbcReceiveResponse,ContractError> {

        let mut state = STATE.load(deps.storage)?;
        // Verify that asset exists in state
        ensure!(
            asset.exists(state.pair),
            ContractError::AssetDoesNotExist {  }
        );

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
        let msg = swap_info.asset.create_transfer_msg(asset_amount, sender.clone());

        Ok(IbcReceiveResponse::new()
        .add_attribute("method", "process_failed_swap")
        .add_attribute("refund_to", "sender")
        .add_attribute("refund_asset", asset.clone().id)
        .add_attribute("refund_amount", asset_amount.clone())
        .add_message(msg)
    )
        }
      

