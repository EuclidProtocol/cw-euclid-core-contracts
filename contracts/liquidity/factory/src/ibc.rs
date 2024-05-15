
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
from_json, to_json_binary, Coin, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcChannel, IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcChannelOpenResponse, IbcOrder, IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse, StdResult, SubMsg, Uint128, WasmMsg
};
use euclid::{error::{ContractError, Never}, pool::{self, extract_sender, Pool}, token::PairInfo};
use euclid_ibc::msg::{AcknowledgementMsg, IbcExecuteMsg, PoolCreationResponse};

use crate::{
    ack::make_ack_fail, msg::PoolInstantiateMsg, reply::INSTANTIATE_REPLY_ID, state::{CONNECTION_COUNTS, POOL_REQUESTS, STATE, TIMEOUT_COUNTS}
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
        IbcExecuteMsg::RequestPoolCreation {pair_info, token_1_reserve, token_2_reserve, pool_rq_id, .. } => {
            // Process acknowledgment for pool creation
            let res: AcknowledgementMsg<PoolCreationResponse> = from_json(ack.acknowledgement.data)?;
            execute_pool_creation(deps, res, pair_info, token_1_reserve, token_2_reserve, pool_rq_id)
        },
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


// Function to create pool 
pub fn execute_pool_creation(
    deps: DepsMut,
    res: AcknowledgementMsg<PoolCreationResponse>,
    pair_info: PairInfo,
    token_1_reserve: Uint128,
    token_2_reserve: Uint128,
    pool_rq_id: String,
) -> Result<IbcBasicResponse, ContractError> {
    // Load the state
    let state = STATE.load(deps.storage)?;
    // Check whether res is an error or not
    match res {
        AcknowledgementMsg::Ok(data) => {
            // Check if the pool was created successfully
             // Prepare Instantiate Msg
            let init_msg = PoolInstantiateMsg {
                vlp_contract: data.vlp_contract.clone(),
                token_pair: pair_info.clone(),
                pool: Pool {
                    chain: state.chain_id.clone(),
                    reserve_1: token_1_reserve.clone(),
                    reserve_2: token_2_reserve.clone(),
                    pair: pair_info.clone(),
                },
                chain_id: state.chain_id.clone(),
            };

            let mut funds = Vec::new();

            // Check for native assets to add to fund
            if pair_info.token_1.is_native() {
                let denom = pair_info.token_1.get_denom();
                let amount = token_1_reserve.clone();
                let coin = Coin { denom, amount };
                funds.push(coin);
            } 

            // Same for tokenm 2
            if pair_info.token_2.is_native() {
                let denom = pair_info.token_2.get_denom();
                let amount = token_2_reserve.clone();
                let coin = Coin { denom, amount };
                funds.push(coin);
            }
            
            let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Instantiate {
                admin: None,
                code_id: state.pool_code_id.clone(),
                msg: to_json_binary(&init_msg).unwrap(),
                funds: funds.clone(), 
                label: "pool".to_string() });
            
            // Create submsg with reply always from msg
            let msg = SubMsg::reply_always(msg, INSTANTIATE_REPLY_ID);
            // Extract sender from rq id
            let sender = extract_sender(pool_rq_id.as_str());
            // Remove pool request from MAP
            POOL_REQUESTS.remove(deps.storage, sender.clone());
            
            Ok(IbcBasicResponse::new()
            .add_attribute("method", "pool_creation")
            .add_submessage(msg)
            )
            },
            
        AcknowledgementMsg::Error(err) => {
            // Process refund of assets for pool creation
            let mut msgs: Vec<CosmosMsg> = Vec::new();

            // Get sender of request
            let sender = extract_sender(pool_rq_id.as_str());

            // Prepare transfer msg for token 1
            let msg_token_1 = pair_info.token_1.create_transfer_msg(token_1_reserve, sender.clone());
            msgs.push(msg_token_1);

            // Prepare transfer msg for token 2
            let msg_token_2 = pair_info.token_2.create_transfer_msg(token_2_reserve, sender.clone());
            msgs.push(msg_token_2);


            // Remove pool request from MAP
            POOL_REQUESTS.remove(deps.storage, sender.clone());

            Ok(IbcBasicResponse::new()
            .add_attribute("method", "refund_pool_request")
            .add_attribute("error", err.clone())
            .add_messages(msgs))
        },
    }
   
}