use std::ops::Add;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, to_json_binary, Addr, Binary, DepsMut, Env, SubMsg, Uint128, WasmMsg};
use euclid::{
    chain::{ChainType, ChainUid},
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::vlp::base::PoolConfig,
    msgs::{factory::interface as factory_interface, router::interface as router_interface},
    recipient::Recipient,
    swap::NextSwapPair,
    token::{Pair, PairWithDenomAndAmount, Token, TokenWithDenom},
};

use crate::state::{
    PendingPacket, NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT,
    NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE, NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE,
    NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER,
};

#[cw_serde]
pub enum RouterCrossChainExecuteMsg {
    // Register Denom for a token
    RegisterDenom {
        sender: CrossChainUser,
        tx_id: String,
        token: TokenWithDenom,
    },

    // Register Denom for a token
    DeregisterDenom {
        sender: CrossChainUser,
        tx_id: String,
        token: TokenWithDenom,
    },
    // Transfer virtual balance message sent from factory
    TransferVoucher(RouterCrossChainTransferVoucherExecuteMsg),

    DepositToken(RouterCrossChainDepositTokenExecuteMsg),
    // Request Pool Creation
    RequestPoolCreation {
        // Factory will set this using info.sender
        sender: CrossChainUser,
        tx_id: String,
        pair: PairWithDenomAndAmount,
        pool_config: PoolConfig,
        // User will provide this data
        slippage_tolerance_bps: u64,
    },
    AddLiquidity {
        // Factory will set this using info.sender
        sender: CrossChainUser,

        // User will provide this data
        slippage_tolerance_bps: u64,

        pair: PairWithDenomAndAmount,

        // Unique per tx
        tx_id: String,
    },

    // Remove liquidity from a chain pool to VLP
    RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg),

    // Swap tokens on VLP
    Swap(RouterCrossChainSwapExecuteMsg),
}

impl RouterCrossChainExecuteMsg {
    pub fn get_tx_id(&self) -> String {
        match self {
            Self::RegisterDenom { tx_id, .. } => tx_id.clone(),
            Self::DeregisterDenom { tx_id, .. } => tx_id.clone(),
            Self::DepositToken(msg) => msg.tx_id.clone(),
            Self::TransferVoucher(msg) => msg.tx_id.clone(),
            Self::RequestPoolCreation { tx_id, .. } => tx_id.clone(),
            Self::AddLiquidity { tx_id, .. } => tx_id.clone(),
            Self::RemoveLiquidity(msg) => msg.tx_id.clone(),
            Self::Swap(msg) => msg.tx_id.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        router_contract: String,
        sender: Addr,
        chain_uid: ChainUid,
        chain_type: ChainType,
        timeout: Option<u64>,
        ack_response: Option<Binary>,
    ) -> Result<SubMsg, ContractError> {
        match chain_type {
            ChainType::Native {} => {
                let router_msg = router_interface::NativeReceiveCallbackMsg::NativeReceiveCallback {
                    msg: to_json_binary(self)?,
                    chain_uid: chain_uid.clone(),
                };
                // Get the current count of the queue
                let mut reply_id = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT
                    .load(deps.storage)
                    .unwrap_or(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0);

                // Wrap around the reply ID range
                if reply_id > NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.1 {
                    reply_id = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0;
                }

                ensure!(
                    !NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.has(deps.storage, reply_id),
                    ContractError::new("Reply ID is already in use")
                );
                NATIVE_CROSS_CHAIN_ORIGINAL_MSG_REPLY_QUEUE.save(
                    deps.storage,
                    reply_id,
                    &PendingPacket {
                        chain_uid,
                        original_msg: to_json_binary(self)?,
                        ack_response,
                    },
                )?;
                NATIVE_CROSS_CHAIN_PENDING_PACKET_SENDER.save(deps.storage, reply_id, &sender)?;
                NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT.save(deps.storage, &reply_id.add(1))?;

                // Return the reply message
                Ok(SubMsg::reply_always(
                    WasmMsg::Execute {
                        contract_addr: router_contract,
                        msg: to_json_binary(&router_msg)?,
                        funds: vec![],
                    },
                    reply_id,
                ))
            }
            ChainType::Cosmos(_) => {
                let factory_internal_msg = factory_interface::SendPacketMsg::SendPacket {
                    msg: to_json_binary(self)?,
                    timeout,
                    ack_response,
                    sender,
                };
                // Trigger a Send Packet execute call to the same contract
                Ok(SubMsg::new(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&factory_internal_msg)?,
                    funds: vec![],
                }))
            }
            _ => Err(ContractError::new(
                "Router only supported on cosmos type chain",
            )),
        }
    }
}

#[cw_serde]
pub struct RouterCrossChainRemoveLiquidityExecuteMsg {
    // Factory will set this using info.sender
    pub sender: CrossChainUser,
    pub lp_allocation: Uint128,
    pub pair: Pair,
    pub recipient: CrossChainUser,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct RouterCrossChainSwapExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,

    // User will provide this
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwapPair>,

    // First element in array has highest priority
    pub recipients: Vec<Recipient>,
    pub partner_fee_amount: Uint128,
    pub partner_fee_recipient: CrossChainUser,

    // Unique per tx
    pub tx_id: String,
}
#[cw_serde]
pub struct RouterCrossChainTransferVoucherExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // User will provide this
    pub token: Token,
    pub amount: Uint128,
    pub from: Option<CrossChainUser>,
    pub recipients: Vec<Recipient>,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct RouterCrossChainDepositTokenExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // User will provide this
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint128,
    pub recipients: Vec<Recipient>,
    // Unique per tx
    pub tx_id: String,
}
