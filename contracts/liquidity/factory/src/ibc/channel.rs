#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, DepsMut, Env, IbcBasicResponse, IbcChannel, IbcChannelCloseMsg, IbcChannelConnectMsg,
    IbcChannelOpenMsg, IbcChannelOpenResponse, IbcOrder,
};
use cw_storage_plus::Map;
use euclid::error::ContractError;

/// (channel_id) -> count. Reset on channel closure.
pub const CONNECTION_COUNTS: Map<String, u32> = Map::new("connection_counts");
/// (channel_id) -> timeout_count. Reset on channel closure.
pub const TIMEOUT_COUNTS: Map<String, u32> = Map::new("timeout_count");

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
