use std::ops::Add;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    ensure, to_json_binary, CosmosMsg, DepsMut, Env, IbcMsg, IbcTimeout, SubMsg, Uint128, WasmMsg,
};
use cw_storage_plus::{Item, Map};
use euclid::{
    chain::{Chain, ChainUid, CrossChainUser, CrossChainUserWithLimit},
    error::ContractError,
    msgs::{factory, router},
    swap::NextSwapPair,
    token::{Pair, Token},
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
        pair: Pair,
    },
    // Request Pool Creation
    RequestEscrowCreation {
        sender: CrossChainUser,
        tx_id: String,
        token: Token,
    },
    AddLiquidity {
        // Factory will set this using info.sender
        sender: CrossChainUser,

        // User will provide this data and factory will verify using info funds
        token_1_liquidity: Uint128,
        token_2_liquidity: Uint128,

        // User will provide this data
        slippage_tolerance: u64,

        pair: Pair,

        // Unique per tx
        tx_id: String,
    },

    // Remove liquidity from a chain pool to VLP
    RemoveLiquidity(ChainIbcRemoveLiquidityExecuteMsg),

    // Swap tokens on VLP
    Swap(ChainIbcSwapExecuteMsg),

    // Withdraw virtual balance message sent from factory
    Withdraw(ChainIbcWithdrawExecuteMsg),
    // RequestWithdraw {
    //     token_id: Token,
    //     amount: Uint128,

    //     // Factory will set this using info.sender
    //     sender: String,

    //     // First element in array has highest priority
    //     cross_chain_addresses: Vec<CrossChainUser>,

    //     // Unique per tx
    //     tx_id: String,
    // },
    // RequestEscrowCreation {
    //     token: Token,
    //     // Factory will set this using info.sender
    //     sender: String,
    //     // Unique per tx
    //     tx_id: String,
    //     //TODO Add allowed denoms?
    // },
}

impl ChainIbcExecuteMsg {
    pub fn get_tx_id(&self) -> String {
        match self {
            Self::AddLiquidity { tx_id, .. } => tx_id.clone(),
            Self::RequestPoolCreation { tx_id, .. } => tx_id.clone(),
            Self::RemoveLiquidity(msg) => msg.tx_id.clone(),
            Self::Swap(msg) => msg.tx_id.clone(),
            Self::Withdraw(msg) => msg.tx_id.clone(),
            Self::RequestEscrowCreation { tx_id, .. } => tx_id.clone(),
        }
    }

    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        router_contract: String,
        chain_uid: ChainUid,
        is_native: bool,
        channel: String,
        timeout: u64,
    ) -> Result<SubMsg, ContractError> {
        if is_native {
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
        } else {
            let packet = IbcMsg::SendPacket {
                channel_id: channel,
                data: to_json_binary(self)?,
                timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
            };
            Ok(SubMsg::new(CosmosMsg::Ibc(packet)))
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
    pub asset_in: Token,
    pub amount_in: Uint128,
    pub asset_out: Token,
    pub min_amount_out: Uint128,
    pub swaps: Vec<NextSwapPair>,

    // First element in array has highest priority
    pub cross_chain_addresses: Vec<CrossChainUserWithLimit>,

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

pub const HUB_IBC_EXECUTE_MSG_QUEUE: Map<u64, HubIbcExecuteMsg> =
    Map::new("hub_ibc_execute_msg_queue");
pub const HUB_IBC_EXECUTE_MSG_QUEUE_COUNT: Item<u64> = Item::new("hub_ibc_execute_msg_queue_count");
pub const HUB_IBC_EXECUTE_MSG_QUEUE_RANGE: (u64, u64) = (1001, 2000);

#[cw_serde]
pub enum HubIbcExecuteMsg {
    // Send Factory Registration Message from Router to Factory
    RegisterFactory {
        chain_uid: ChainUid,
        // Unique per tx
        tx_id: String,
    },

    ReleaseEscrow {
        chain_uid: ChainUid,
        sender: CrossChainUser,
        amount: Uint128,
        token: Token,
        to_address: String,

        // Unique per tx
        tx_id: String,
    },
}

impl HubIbcExecuteMsg {
    pub fn get_tx_id(&self) -> String {
        match self {
            Self::RegisterFactory { tx_id, .. } => tx_id.clone(),
            Self::ReleaseEscrow { tx_id, .. } => tx_id.clone(),
        }
    }

    pub fn to_msg(
        &self,
        deps: &mut DepsMut,
        env: &Env,
        chain: Chain,
        timeout: u64,
    ) -> Result<SubMsg, ContractError> {
        match chain.chain_type {
            euclid::chain::ChainType::Ibc(ibc_info) => {
                let packet = IbcMsg::SendPacket {
                    channel_id: ibc_info.from_hub_channel,
                    data: to_json_binary(self)?,
                    timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
                };
                Ok(SubMsg::new(CosmosMsg::Ibc(packet)))
            }
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
        }
    }
}
