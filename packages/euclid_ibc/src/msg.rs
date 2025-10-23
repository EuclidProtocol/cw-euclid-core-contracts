use std::ops::Add;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, to_json_binary, Binary, Coin, DepsMut, Env, SubMsg, Uint128, WasmMsg};
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::{Chain, ChainType, ChainUid, CrossChainUser, CrossChainUserWithLimit},
    error::ContractError,
    msgs::{factory, router},
    pool::PoolConfig,
    swap::NextSwapPair,
    token::{Pair, PairWithDenomAndAmount, Token, TokenWithDenom},
    utils::fund_manager::FundManager,
};

// Message that implements an ExecuteSwap on the VLP contract

pub const CHAIN_IBC_EXECUTE_MSG_QUEUE: Map<u64, ChainIbcExecuteMsg> =
    Map::new("chain_ibc_execute_msg_queue");
pub const CHAIN_IBC_EXECUTE_MSG_QUEUE_COUNT: Item<u64> =
    Item::new("chain_ibc_execute_msg_queue_count");
pub const CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE: (u64, u64) = (2001, 3000);

#[cw_serde]
pub enum ChainIbcExecuteMsg {
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
    // Register Denom for a token
    RegisterDenom {
        sender: CrossChainUser,
        tx_id: String,
        token: TokenWithDenom,
    },

    // Register Denom for a token
    DeRegisterDenom {
        sender: CrossChainUser,
        tx_id: String,
        token: TokenWithDenom,
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
    RemoveLiquidity(ChainIbcRemoveLiquidityExecuteMsg),

    // Swap tokens on VLP
    Swap(ChainIbcSwapExecuteMsg),

    // Withdraw virtual balance message sent from factory
    Withdraw(ChainIbcWithdrawExecuteMsg),

    // Transfer virtual balance message sent from factory
    Transfer(ChainIbcTransferExecuteMsg),

    DepositToken(ChainIbcDepositTokenExecuteMsg),
}

impl ChainIbcExecuteMsg {
    pub fn get_tx_id(&self) -> String {
        match self {
            Self::AddLiquidity { tx_id, .. } => tx_id.clone(),
            Self::RequestPoolCreation { tx_id, .. } => tx_id.clone(),
            Self::RemoveLiquidity(msg) => msg.tx_id.clone(),
            Self::Swap(msg) => msg.tx_id.clone(),
            Self::Withdraw(msg) => msg.tx_id.clone(),
            Self::DepositToken(msg) => msg.tx_id.clone(),
            Self::RegisterDenom { tx_id, .. } => tx_id.clone(),
            Self::DeRegisterDenom { tx_id, .. } => tx_id.clone(),
            Self::Transfer(msg) => msg.tx_id.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        router_contract: String,
        chain_uid: ChainUid,
        chain_type: ChainType,
        sender: String,
        _timeout: u64,
        funds_manager: FundManager,
    ) -> Result<SubMsg, ContractError> {
        match chain_type {
            ChainType::Native {} => {
                let router_msg = router::ExecuteMsg::NativeReceiveCallback {
                    msg: to_json_binary(self)?,
                    chain_uid,
                };
                let mut count = CHAIN_IBC_EXECUTE_MSG_QUEUE_COUNT
                    .load(deps.storage)
                    .unwrap_or(CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE.0);

                count = count
                    .min(CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE.1)
                    .max(CHAIN_IBC_EXECUTE_MSG_QUEUE_RANGE.0);

                ensure!(
                    !CHAIN_IBC_EXECUTE_MSG_QUEUE.has(deps.storage, count),
                    ContractError::new("Msg Queue is full")
                );
                CHAIN_IBC_EXECUTE_MSG_QUEUE.save(deps.storage, count, self)?;

                CHAIN_IBC_EXECUTE_MSG_QUEUE_COUNT.save(deps.storage, &count.add(1))?;

                Ok(SubMsg::reply_always(
                    WasmMsg::Execute {
                        contract_addr: router_contract,
                        msg: to_json_binary(&router_msg)?,
                        funds: vec![],
                    },
                    count,
                ))
            }

            // Temporary solution for cosmos relaying
            ChainType::Ibc(_ibc_info) => {
                let factory_internal_msg = factory::ExecuteMsg::CosmosSendPacket {
                    msg: to_json_binary(self)?,
                    cross_chain_user: CrossChainUser {
                        chain_uid,
                        address: sender,
                    },
                };

                let (denom, amount) = funds_manager.get_single_fund()?;
                // Trigger a Send Packet execute call to the same contract
                Ok(SubMsg::new(WasmMsg::Execute {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_json_binary(&factory_internal_msg)?,
                    funds: vec![Coin::new(amount.u128(), denom)],
                }))
            }
            _ => Err(ContractError::new("Unsupported chain type")),
        }
    }
}

#[cw_serde]
pub struct ChainIbcRemoveLiquidityExecuteMsg {
    // Factory will set this using info.sender
    pub sender: CrossChainUser,
    pub lp_allocation: Uint128,
    pub pair: Pair,
    // First element in array has highest priority
    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct ChainIbcSwapExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,

    // User will provide this
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwapPair>,

    // First element in array has highest priority
    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    pub partner_fee_amount: Uint128,
    pub partner_fee_recipient: CrossChainUser,

    // Unique per tx
    pub tx_id: String,
}

#[cw_serde]
pub struct ChainIbcWithdrawExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // User will provide this
    pub token: Token,
    pub amount: Uint128,
    // First element in array has highest priority
    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,
    // Unique per tx
    pub tx_id: String,
    pub timeout: Option<u64>,
}

#[cw_serde]
pub struct ChainIbcTransferExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // User will provide this
    pub token: Token,
    pub amount: Uint128,
    pub recipient_address: CrossChainUser,
    pub from: Option<CrossChainUser>,
    pub msg: Option<Binary>,
    // Unique per tx
    pub tx_id: String,
    pub timeout: Option<u64>,
}

#[cw_serde]
pub struct ChainIbcDepositTokenExecuteMsg {
    // Factory will set this to info.sender
    pub sender: CrossChainUser,
    // User will provide this
    pub asset_in: TokenWithDenom,
    pub amount_in: Uint128,
    pub recipient: CrossChainUser,
    pub msg: Option<Binary>,
    // Unique per tx
    pub tx_id: String,
}

pub const HUB_IBC_EXECUTE_MSG_QUEUE: Map<u64, HubIbcExecuteMsg> =
    Map::new("hub_ibc_execute_msg_queue");
pub const HUB_IBC_EXECUTE_MSG_QUEUE_COUNT: Item<u64> = Item::new("hub_ibc_execute_msg_queue_count");
pub const HUB_IBC_EXECUTE_MSG_QUEUE_RANGE: (u64, u64) = (1001, 2000);

#[cw_serde]
pub enum HubIbcExecuteMsg {
    // Send Factory Registration Message from Router to Factory
    RegisterFactory {
        chain_uid: ChainUid,
        chain_type: ChainType,
        // Unique per tx
        tx_id: String,
    },

    UpdateFactoryChannel {
        chain_uid: ChainUid,
        chain_type: ChainType,
        // Unique per tx
        tx_id: String,
    },

    ReleaseEscrow {
        sender: CrossChainUser,
        amount: Uint128,
        recipient: CrossChainUserWithLimit,
        token: Token,
        // Unique per tx
        tx_id: String,
    },
}

impl HubIbcExecuteMsg {
    pub fn get_tx_id(&self) -> String {
        match self {
            Self::RegisterFactory { tx_id, .. } => tx_id.clone(),
            Self::ReleaseEscrow { tx_id, .. } => tx_id.clone(),
            Self::UpdateFactoryChannel { tx_id, .. } => tx_id.clone(),
        }
    }

    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        chain_uid: ChainUid,
        chain: Chain,
        _timeout: u64,
    ) -> Result<SubMsg, ContractError> {
        match chain.chain_type {
            // Temporary solution for cosmos chain speed
            euclid::chain::ChainType::Native {} => {
                let factory_msg = factory::ExecuteMsg::NativeReceiveCallback {
                    msg: to_json_binary(self)?,
                };
                let mut count = HUB_IBC_EXECUTE_MSG_QUEUE_COUNT
                    .load(deps.storage)
                    .unwrap_or(HUB_IBC_EXECUTE_MSG_QUEUE_RANGE.0);

                count = count
                    .min(HUB_IBC_EXECUTE_MSG_QUEUE_RANGE.1)
                    .max(HUB_IBC_EXECUTE_MSG_QUEUE_RANGE.0);

                ensure!(
                    !HUB_IBC_EXECUTE_MSG_QUEUE.has(deps.storage, count),
                    ContractError::new("Msg Queue is full")
                );
                HUB_IBC_EXECUTE_MSG_QUEUE.save(deps.storage, count, self)?;

                HUB_IBC_EXECUTE_MSG_QUEUE_COUNT.save(deps.storage, &count.add(1))?;

                Ok(SubMsg::reply_always(
                    WasmMsg::Execute {
                        contract_addr: chain.factory,
                        msg: to_json_binary(&factory_msg)?,
                        funds: vec![],
                    },
                    count,
                ))
            }
            _ => {
                let router_internal_msg = router::ExecuteMsg::SendPacket {
                    msg: to_json_binary(self)?,
                    chain_uid,
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
