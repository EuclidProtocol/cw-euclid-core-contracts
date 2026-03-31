use std::ops::Add;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, to_json_binary, Addr, Binary, DepsMut, Env, SubMsg, Uint128, WasmMsg};
use euclid::{
    chain::{Chain, ChainUid},
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::{
        factory::interface as factory_interface,
        router::{interface as router_interface, RegisterFactoryChainType},
    },
    token::{Token, TokenType},
};

use crate::state::{
    PendingPacket, NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT,
    NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE, NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE,
    NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER,
};

#[cw_serde]
pub enum FactoryCrossChainExecuteMsg {
    // Send Factory Registration Message from Router to Factory
    RegisterFactory {
        chain_uid: ChainUid,
        chain_type: RegisterFactoryChainType,
        // Unique per tx
        tx_id: String,
    },

    ReleaseEscrow {
        sender: CrossChainUser,
        token: Token,
        recipient: String,
        amount: Uint128,
        denom: TokenType,
        forwarding_message: Option<String>,
        // Unique per tx
        tx_id: String,
    },
}

impl FactoryCrossChainExecuteMsg {
    pub fn get_tx_id(&self) -> String {
        match self {
            Self::RegisterFactory { tx_id, .. } => tx_id.clone(),
            Self::ReleaseEscrow { tx_id, .. } => tx_id.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        sender: String,
        chain: Chain,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
    ) -> Result<SubMsg, ContractError> {
        match chain.chain_type {
            euclid::chain::ChainType::Native {} => {
                let factory_msg = factory_interface::NativeReceiveCallbackMsg::NativeReceiveCallback {
                    msg: to_json_binary(self)?,
                };
                let mut count = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT
                    .load(deps.storage)
                    .unwrap_or(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0);

                count = count
                    .min(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.1)
                    .max(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0);

                ensure!(
                    !NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.has(deps.storage, count),
                    ContractError::new("Msg Queue is full")
                );
                NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.save(
                    deps.storage,
                    count,
                    &PendingPacket {
                        chain_uid: chain.chain_uid.clone(),
                        original_msg: to_json_binary(self)?,
                        ack_response,
                    },
                )?;
                NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER.save(
                    deps.storage,
                    count,
                    &Addr::unchecked(sender),
                )?;

                NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT.save(deps.storage, &count.add(1))?;

                Ok(SubMsg::reply_always(
                    WasmMsg::Execute {
                        contract_addr: chain.factory_address.clone(),
                        msg: to_json_binary(&factory_msg)?,
                        funds: vec![],
                    },
                    count,
                ))
            }
            _ => {
                let router_internal_msg = router_interface::SendPacketMsg::SendPacket {
                    msg: to_json_binary(self)?,
                    chain,
                    timeout,
                    ack_response,
                    sender,
                };
                // Trigger a Send Packet execute call to the same contract
                Ok(SubMsg::new(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&router_internal_msg)?,
                    funds: vec![],
                }))
            }
        }
    }
}
