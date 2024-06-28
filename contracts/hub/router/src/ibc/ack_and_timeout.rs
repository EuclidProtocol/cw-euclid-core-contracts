#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, CosmosMsg, DepsMut, Env, IbcBasicResponse, IbcPacketAckMsg, IbcPacketTimeoutMsg,
    StdResult, Uint128, WasmMsg,
};
use cosmwasm_std::{to_json_binary, IbcAcknowledgement};
use euclid::error::ContractError;
use euclid::msgs::factory::{RegisterFactoryResponse, ReleaseEscrowResponse};
use euclid::msgs::router::Chain;
use euclid::msgs::vcoin::{ExecuteMint, ExecuteMsg as VcoinExecuteMsg};
use euclid::vcoin::BalanceKey;
use euclid_ibc::msg::{AcknowledgementMsg, HubIbcExecuteMsg};

use crate::state::{CHAIN_ID_TO_CHAIN, CHANNEL_TO_CHAIN_ID, STATE};

use super::channel::TIMEOUT_COUNTS;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn ibc_packet_ack(
    deps: DepsMut,
    env: Env,
    ack: IbcPacketAckMsg,
) -> Result<IbcBasicResponse, ContractError> {
    // Parse the ack based on request
    let msg: HubIbcExecuteMsg = from_json(ack.original_packet.data)?;
    match msg {
        HubIbcExecuteMsg::RegisterFactory { .. } => {
            let res = from_json(ack.acknowledgement.data)?;
            ibc_ack_register_factory(
                deps,
                env,
                ack.original_packet.src.channel_id,
                ack.original_packet.dest.channel_id,
                res,
            )
        }
        HubIbcExecuteMsg::ReleaseEscrow {
            amount,
            token_id,
            to_address,
            to_chain_uid,
        } => {
            let res = from_json(ack.acknowledgement.data)?;
            ibc_ack_release_escrow(deps, env, amount, token_id, to_address, to_chain_uid, res)
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
    _env: Env,
    from_hub_channel: String,
    from_factory_channel: String,
    res: AcknowledgementMsg<RegisterFactoryResponse>,
) -> Result<IbcBasicResponse, ContractError> {
    match res {
        AcknowledgementMsg::Ok(data) => {
            CHANNEL_TO_CHAIN_ID.save(deps.storage, from_hub_channel.clone(), &data.chain_id)?;
            let chain_data = Chain {
                factory_chain_id: data.chain_id.clone(),
                factory: data.factory_address.clone(),
                from_hub_channel,
                from_factory_channel,
            };
            CHAIN_ID_TO_CHAIN.save(deps.storage, data.chain_id.clone(), &chain_data)?;
            Ok(IbcBasicResponse::new()
                .add_attribute("method", "register_factory_ack_success")
                .add_attribute("factory_chain", data.chain_id)
                .add_attribute("factory_address", data.factory_address))
        }

        AcknowledgementMsg::Error(err) => Ok(IbcBasicResponse::new()
            .add_attribute("method", "register_factory_ack_error")
            .add_attribute("error", err.clone())),
    }
}

pub fn ibc_ack_release_escrow(
    deps: DepsMut,
    _env: Env,
    amount: Uint128,
    token_id: String,
    to_address: String,
    to_chain_uid: String,
    res: AcknowledgementMsg<ReleaseEscrowResponse>,
) -> Result<IbcBasicResponse, ContractError> {
    match res {
        AcknowledgementMsg::Ok(data) => Ok(IbcBasicResponse::new()
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

            // TODO: Revisit this as this should be original sender in case of multiple escrow release on different chain
            let mint_msg = VcoinExecuteMsg::Mint(ExecuteMint {
                amount,
                balance_key: BalanceKey {
                    chain_uid: to_chain_uid,
                    address: to_address,
                    token_id,
                },
            });
            let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: vcoin_address.into_string(),
                msg: to_json_binary(&mint_msg)?,
                funds: vec![],
            });

            Ok(IbcBasicResponse::new()
                .add_message(msg)
                .add_attribute("method", "escrow_release_ack")
                .add_attribute("error", err)
                .add_attribute("mint_amount", "value")
                .add_attribute("balance_key", "balance_key"))
        }
    }
}
