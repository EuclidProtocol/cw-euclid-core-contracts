use std::ops::Add;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure, to_json_binary, Addr, Binary, DepsMut, Env, SubMsg, Uint128, Uint256, WasmMsg,
};
use euclid::{
    chain::{ChainType, ChainUid},
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::vlp::base::{PoolConfig, PoolKey},
    msgs::{factory, router},
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
    RequestConcentratedPoolCreation(RouterCrossChainConcentratedRequestPoolCreationExecuteMsg),
    AddLiquidity {
        // Factory will set this using info.sender
        sender: CrossChainUser,

        // User will provide this data
        slippage_tolerance_bps: u64,

        pair: PairWithDenomAndAmount,

        // Unique per tx
        tx_id: String,
    },
    AddConcentratedLiquidity(RouterCrossChainConcentratedAddLiquidityExecuteMsg),

    // Remove liquidity from a chain pool to VLP
    RemoveLiquidity(RouterCrossChainRemoveLiquidityExecuteMsg),
    RemoveConcentratedLiquidity(RouterCrossChainConcentratedRemoveLiquidityExecuteMsg),
    CollectConcentratedFees(RouterCrossChainConcentratedCollectFeesExecuteMsg),
    CollectConcentratedProtocolFees(RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg),

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
            Self::RequestConcentratedPoolCreation(msg) => msg.tx_id.clone(),
            Self::AddLiquidity { tx_id, .. } => tx_id.clone(),
            Self::AddConcentratedLiquidity(msg) => msg.tx_id.clone(),
            Self::RemoveLiquidity(msg) => msg.tx_id.clone(),
            Self::RemoveConcentratedLiquidity(msg) => msg.tx_id.clone(),
            Self::CollectConcentratedFees(msg) => msg.tx_id.clone(),
            Self::CollectConcentratedProtocolFees(msg) => msg.tx_id.clone(),
            Self::Swap(msg) => msg.tx_id.clone(),
        }
    }

    /// Returns true if `self` is a pool-related variant currently owned by
    /// `pool_factory`. Used in two places that MUST stay in lockstep:
    ///   1. The inbound ack dispatcher on main factory, to decide whether to
    ///      forward an ack to `pool_factory::OnPoolAck`.
    ///   2. The outbound reply handler on main factory (post-`ProxySendPacket`
    ///      removal), to reject any non-pool packet returned by `pool_factory`
    ///      as defence in depth.
    ///
    /// Extended slice-by-slice as additional pool flows are delegated. Adding
    /// a new variant here without also retrofitting both sites will cause
    /// either an unrouted ack (false negative) or an unsendable packet
    /// (false positive); reviewers should confirm both sites match.
    pub fn is_pool_variant(&self) -> bool {
        matches!(
            self,
            Self::RequestPoolCreation { .. }
                | Self::RequestConcentratedPoolCreation(_)
                | Self::AddLiquidity { .. }
                | Self::RemoveLiquidity(_)
        )
    }

    /// Returns a reference to the sender CrossChainUser from any variant.
    pub fn get_sender(&self) -> &CrossChainUser {
        match self {
            Self::RegisterDenom { sender, .. } => sender,
            Self::DeregisterDenom { sender, .. } => sender,
            Self::DepositToken(msg) => &msg.sender,
            Self::TransferVoucher(msg) => &msg.sender,
            Self::RequestPoolCreation { sender, .. } => sender,
            Self::AddLiquidity { sender, .. } => sender,
            Self::RemoveLiquidity(msg) => &msg.sender,
            Self::Swap(msg) => &msg.sender,
            Self::RequestConcentratedPoolCreation(msg) => &msg.sender,
            Self::AddConcentratedLiquidity(msg) => &msg.sender,
            Self::RemoveConcentratedLiquidity(msg) => &msg.sender,
            Self::CollectConcentratedFees(msg) => &msg.sender,
            Self::CollectConcentratedProtocolFees(msg) => &msg.sender,
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
                let router_msg = router::execute::ExecuteMsg::NativeReceiveCallback {
                    msg: to_json_binary(self)?,
                    chain_uid: chain_uid.clone(),
                };
                // Advance the counter within the reserved range (2001–3000).
                // When it exceeds 3000, wrap back to 2001 for slot reuse.
                // The `ensure!` below is the hard guard: if the slot is still occupied
                // (i.e. all 1 000 slots are in-flight simultaneously), the call errors.
                let mut reply_id = NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_COUNT
                    .load(deps.storage)
                    .unwrap_or(NATIVE_CROSS_CHAIN_MSG_REPLY_QUEUE_RANGE.0);

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
                let factory_internal_msg = factory::msg::ExecuteMsg::SendPacket {
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
    pub lp_allocation: Uint256,
    pub pair: Pair,
    pub recipient: CrossChainUser,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct RouterCrossChainConcentratedRequestPoolCreationExecuteMsg {
    pub sender: CrossChainUser,
    pub tx_id: String,
    pub pair: PairWithDenomAndAmount,
    pub pool_key: PoolKey,
    pub slippage_tolerance_bps: u64,
    /// Initial tick for the pool price. `None` means tick 0 (1:1 price).
    pub initial_tick: Option<i64>,
}

#[cw_serde]
pub struct RouterCrossChainConcentratedAddLiquidityExecuteMsg {
    pub sender: CrossChainUser,
    pub pair: PairWithDenomAndAmount,
    pub pool_key: PoolKey,
    pub lower_tick_index: i64,
    pub upper_tick_index: i64,
    pub position_id: Option<Uint128>,
    pub slippage_tolerance_bps: u64,
    pub tx_id: String,
}

#[cw_serde]
pub struct RouterCrossChainConcentratedRemoveLiquidityExecuteMsg {
    pub sender: CrossChainUser,
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub liquidity_delta: Uint128,
    pub recipient: CrossChainUser,
    pub tx_id: String,
}

#[cw_serde]
pub struct RouterCrossChainConcentratedCollectFeesExecuteMsg {
    pub sender: CrossChainUser,
    pub pool_key: PoolKey,
    pub position_id: Uint128,
    pub recipient: CrossChainUser,
    pub tx_id: String,
}

#[cw_serde]
pub struct RouterCrossChainConcentratedCollectProtocolFeesExecuteMsg {
    pub sender: CrossChainUser,
    pub pool_key: PoolKey,
    pub recipient: CrossChainUser,
    pub amount_0_requested: Uint128,
    pub amount_1_requested: Uint128,
    pub tx_id: String,
}

#[cw_serde]
pub struct RouterCrossChainSwapExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,

    // User will provide this
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint256,
    pub asset_out: Token,
    pub min_amount_out: Uint256,
    pub swaps: Vec<NextSwapPair>,

    // First element in array has highest priority
    pub recipients: Vec<Recipient>,
    pub partner_fee_amount: Uint256,
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
    pub amount: Uint256,
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
    pub amount_in: Uint256,
    pub recipients: Vec<Recipient>,
    // Unique per tx
    pub tx_id: String,
}
