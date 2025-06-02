// use crate::ack::{make_ack_fail, make_ack_success};
use crate::execute;
// use crate::state::{CHAIN_TO_CHANNEL, CHANNEL_TO_CHAIN, KERNEL_ADDRESSES, REFUND_DATA};
use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, from_json, to_json_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Empty, Env,
    Ibc3ChannelOpenResponse, IbcBasicResponse, IbcChannel, IbcChannelCloseMsg,
    IbcChannelConnectMsg, IbcChannelOpenMsg, IbcOrder, IbcPacketAckMsg, IbcPacketReceiveMsg,
    IbcPacketTimeoutMsg, IbcReceiveResponse, MessageInfo, Never, SubMsg, WasmMsg,
};
use euclid::{error::ContractError, msgs::migrator::IbcExecuteMsg};

pub const PACKET_LIFETIME: u64 = 604_800u64;

#[cw_serde]
pub enum Ack {
    Result(Binary),
    Error(String),
}

pub fn make_ack_success() -> Binary {
    let res = Ack::Result(b"1".into());
    to_json_binary(&res).unwrap()
}

pub fn make_ack_fail(err: String) -> Binary {
    let res = Ack::Error(err);
    to_json_binary(&res).unwrap()
}

#[cw_serde]
pub enum IBCLifecycleComplete {
    #[serde(rename = "ibc_ack")]
    IBCAck {
        channel: String,
        sequence: u64,
        ack: String,
        success: bool,
    },
    #[serde(rename = "ibc_timeout")]
    IBCTimeout { channel: String, sequence: u64 },
}

#[cw_serde]
pub enum SudoMsg {
    #[serde(rename = "ibc_lifecycle_complete")]
    IBCLifecycleComplete(IBCLifecycleComplete),
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_timeout(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketTimeoutMsg,
) -> Result<IbcBasicResponse, ContractError> {
    // As with ack above, nothing to do here. If we cared about
    // keeping track of state between the two chains then we'd want to
    // respond to this likely as it means that the packet in question
    // isn't going anywhere.
    Ok(IbcBasicResponse::new().add_attribute("method", "ibc_packet_timeout"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_open(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelOpenMsg,
) -> Result<Option<Ibc3ChannelOpenResponse>, ContractError> {
    validate_order_and_version(msg.channel(), msg.counterparty_version())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_connect(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelConnectMsg,
) -> Result<IbcBasicResponse, ContractError> {
    validate_order_and_version(msg.channel(), msg.counterparty_version())?;

    let channel = msg.channel().endpoint.channel_id.clone();

    Ok(IbcBasicResponse::new()
        .add_attribute("method", "ibc_channel_connect")
        .add_attribute("channel_id", channel))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_channel_close(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelCloseMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let channel = msg.channel().endpoint.channel_id.clone();
    // Reset the state for the channel.
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
            .set_ack(make_ack_fail(error.to_string()))
            .add_attribute("error", error.to_string())),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    Ok(IbcBasicResponse::new())
}
pub fn do_ibc_packet_receive(
    mut deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, ContractError> {
    let channel = msg.clone().packet.dest.channel_id;
    let packet_msg: IbcExecuteMsg = from_json(&msg.packet.data)?;

    match packet_msg {
        IbcExecuteMsg::MigrateVBalance {
            vbalance_address,
            migrate_msg,
        } => {
            let vbalance_execute_msg =
                euclid::msgs::virtual_balance::ExecuteMsg::MigrateVBalance(migrate_msg);

            let msg = SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: vbalance_address,
                msg: to_json_binary(&vbalance_execute_msg)?,
                funds: vec![],
            }));

            Ok(IbcReceiveResponse::new().add_submessage(msg))
        }
    }
}

pub fn validate_order_and_version(
    channel: &IbcChannel,
    counterparty_version: Option<&str>,
) -> Result<Option<Ibc3ChannelOpenResponse>, ContractError> {
    // We expect an unordered channel here. Ordered channels have the
    // property that if a message is lost the entire channel will stop
    // working until you start it again.
    ensure!(
        channel.order == IbcOrder::Unordered,
        ContractError::OrderedChannel {}
    );
    // ensure!(
    //     channel.version == IBC_VERSION || channel.version == "ics20-1",
    //     ContractError::InvalidVersion {
    //         actual: channel.version.to_string(),
    //         expected: IBC_VERSION.to_string(),
    //     }
    // );

    // Make sure that we're talking with a counterparty who speaks the
    // same "protocol" as us.
    //
    // For a connection between chain A and chain B being established
    // by chain A, chain B knows counterparty information during
    // `OpenTry` and chain A knows counterparty information during
    // `OpenAck`. We verify it when we have it but when we don't it's
    // alright.
    // if let Some(counterparty_version) = counterparty_version {
    //     ensure!(
    //         counterparty_version == IBC_VERSION || channel.version == "ics20-1",
    //         ContractError::InvalidVersion {
    //             actual: counterparty_version.to_string(),
    //             expected: IBC_VERSION.to_string(),
    //         }
    //     );
    // }

    Ok(Some(Ibc3ChannelOpenResponse {
        version: channel.version.to_string(),
    }))
}
