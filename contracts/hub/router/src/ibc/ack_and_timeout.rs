#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcPacketAckMsg, IbcPacketTimeoutMsg,
    Response, StdResult, SubMsg, Uint128, WasmMsg,
};
use cosmwasm_std::{to_json_binary, IbcAcknowledgement};
use euclid::chain::{ChainUid, CrossChainUser};
use euclid::error::ContractError;
use euclid::events::{tx_event, TxType};
use euclid::msgs::factory::{RegisterFactoryResponse, ReleaseEscrowResponse};
use euclid::msgs::router::{Chain, ExecuteMsg};
use euclid::msgs::vcoin::{ExecuteMint, ExecuteMsg as VcoinExecuteMsg};
use euclid::token::Token;
use euclid::vcoin::BalanceKey;
use euclid_ibc::ack::AcknowledgementMsg;
use euclid_ibc::msg::HubIbcExecuteMsg;

use crate::reply::IBC_ACK_AND_TIMEOUT_REPLY_ID;
use crate::state::{CHAIN_UID_TO_CHAIN, CHANNEL_TO_CHAIN_UID, ESCROW_BALANCES, STATE};

use super::channel::TIMEOUT_COUNTS;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    _deps: DepsMut,
    env: Env,
    ack: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    let internal_msg = ExecuteMsg::IbcCallbackAckAndTimeout { ack: ack.clone() };
    let internal_msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_json_binary(&internal_msg)?,
        funds: vec![],
    });
    let sub_msg = SubMsg::reply_always(internal_msg, IBC_ACK_AND_TIMEOUT_REPLY_ID);
    Ok(IbcBasicResponse::new()
        .add_attribute("ibc_ack", ack.acknowledgement.data.to_string())
        .add_submessage(sub_msg))
}

pub fn ibc_ack_packet_internal_call(
    deps: DepsMut,
    env: Env,
    ack: IbcPacketAckMsg,
) -> Result<Response, ContractError> {
    // Parse the ack based on request
    let msg: HubIbcExecuteMsg = from_json(ack.original_packet.data)?;
    match msg {
        HubIbcExecuteMsg::RegisterFactory {
            chain_uid, tx_id, ..
        } => {
            let res = from_json(ack.acknowledgement.data)?;
            ibc_ack_register_factory(
                deps,
                env,
                chain_uid,
                ack.original_packet.src.channel_id,
                ack.original_packet.dest.channel_id,
                res,
                tx_id,
            )
        }
        HubIbcExecuteMsg::ReleaseEscrow {
            amount,
            token,
            tx_id,
            sender,
            ..
        } => {
            let chain_uid =
                CHANNEL_TO_CHAIN_UID.load(deps.storage, ack.original_packet.src.channel_id)?;
            let res = from_json(ack.acknowledgement.data)?;
            ibc_ack_release_escrow(deps, env, chain_uid, sender, amount, token, res, tx_id)
        }
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
pub fn ibc_ack_register_factory(
    deps: DepsMut,
    env: Env,
    chain_uid: ChainUid,
    from_hub_channel: String,
    from_factory_channel: String,
    res: AcknowledgementMsg<RegisterFactoryResponse>,
    tx_id: String,
) -> Result<Response, ContractError> {
    let response = Response::new().add_event(tx_event(
        &tx_id,
        env.contract.address.as_str(),
        TxType::RegisterFactory,
    ));
    match res {
        AcknowledgementMsg::Ok(data) => {
            CHANNEL_TO_CHAIN_UID.save(deps.storage, from_hub_channel.clone(), &chain_uid)?;
            let chain_data = Chain {
                factory_chain_id: data.chain_id.clone(),
                factory: data.factory_address.clone(),
                from_hub_channel,
                from_factory_channel,
            };
            CHAIN_UID_TO_CHAIN.save(deps.storage, chain_uid.clone(), &chain_data)?;
            Ok(response
                .add_attribute("method", "register_factory_ack_success")
                .add_attribute("factory_chain", data.chain_id)
                .add_attribute("factory_address", data.factory_address))
        }

        AcknowledgementMsg::Error(err) => Ok(response
            .add_attribute("method", "register_factory_ack_error")
            .add_attribute("error", err.clone())),
    }
}

pub fn ibc_ack_release_escrow(
    deps: DepsMut,
    _env: Env,
    chain_uid: ChainUid,
    sender: CrossChainUser,
    amount: Uint128,
    token: Token,
    res: AcknowledgementMsg<ReleaseEscrowResponse>,
    tx_id: String,
) -> Result<Response, ContractError> {
    let response = Response::new().add_event(tx_event(
        &tx_id,
        sender.address.as_str(),
        TxType::EscrowRelease,
    ));
    match res {
        AcknowledgementMsg::Ok(data) => Ok(response
            .add_attribute("method", "release_escrow_success")
            .add_attribute("factory_chain", data.chain_id)
            .add_attribute("factory_address", data.factory_address)),
        // Re-mint tokens
        AcknowledgementMsg::Error(err) => {
            let vcoin_address =
                STATE
                    .load(deps.storage)?
                    .vcoin_address
                    .ok_or(ContractError::Generic {
                        err: "Vcoin not available".to_string(),
                    })?;

            // Escrow release failed, mint tokens again for the original cross chain sender
            let mint_msg = VcoinExecuteMsg::Mint(ExecuteMint {
                amount,
                balance_key: BalanceKey {
                    cross_chain_user: sender,
                    token_id: token.to_string(),
                },
            });
            let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: vcoin_address.into_string(),
                msg: to_json_binary(&mint_msg)?,
                funds: vec![],
            });

            // Escrow release is failed, add the old escrow balance again
            let escrow_key = ESCROW_BALANCES.key((token, chain_uid));
            let new_balance = escrow_key.load(deps.storage)?.checked_add(amount)?;
            escrow_key.save(deps.storage, &new_balance)?;

            Ok(response
                .add_message(msg)
                .add_attribute("method", "escrow_release_ack")
                .add_attribute("error", err)
                .add_attribute("mint_amount", "value")
                .add_attribute("balance_key", "balance_key"))
        }
    }
}
