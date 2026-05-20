use std::ops::Add;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, to_json_binary, Addr, Binary, DepsMut, Env, SubMsg, Uint256, WasmMsg};
use euclid::{
    chain::{ChainType, ChainUid},
    cross_chain_user::CrossChainUser,
    error::ContractError,
    msgs::vlp::base::PoolConfig,
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

    // Single-sided add liquidity: deposit one token, hub atomically swaps a portion
    // and adds liquidity on the same VLP
    SingleSidedAddLiquidity(RouterCrossChainSingleSidedAddLiquidityMsg),
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
            Self::SingleSidedAddLiquidity(msg) => msg.tx_id.clone(),
        }
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
            Self::SingleSidedAddLiquidity(msg) => &msg.sender,
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
pub struct RouterCrossChainSingleSidedAddLiquidityMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // The single token the user is depositing
    pub asset_in: TokenWithDenom,
    // Total raw amount of asset_in AFTER partner-fee deduction.
    // This is the amount the hub operates on; the partner-fee portion never crosses IBC.
    pub amount_in: Uint256,
    // Raw amount of asset_in to swap into the other side of the pair (backend-computed)
    pub swap_amount: Uint256,
    // Target VLP pair. The "other" token (asset_out for the swap leg) is
    // derived as pair.get_other_token(asset_in.token).
    pub pair: Pair,
    // Swap route. v1: must be length 1; kept Vec for forward-compat.
    pub swaps: Vec<NextSwapPair>,
    // Minimum LP tokens to receive — sole user-facing slippage guard
    pub min_lp_out: Uint256,
    // Partner-fee accounting (used only by the factory ack handler).
    pub partner_fee_amount: Uint256,
    pub partner_fee_recipient: CrossChainUser,
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
